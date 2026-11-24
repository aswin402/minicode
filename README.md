<a id="readme-top"></a>

<div align="center">

[![Release][release-shield]][release-url]
[![CI][ci-shield]][ci-url]
[![Rust][rust-shield]][rust-url]
[![Platforms][platforms-shield]][release-url]
[![License][license-shield]][license-url]
[![Stars][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]

</div>

<br />
<div align="center">
  <a href="https://github.com/aswin402/minicode">
    <img src="assets/logo.png" alt="minicode mascot logo" width="180" height="180">
  </a>

  <h1 align="center">minicode</h1>

  <p align="center">
    <strong>Fast, minimalist Rust-native TUI + CLI AI coding agent built for humans and autonomous agents.</strong>
    <br />
    Zero Node/Python runtime overhead · 215 built-in tools · 23 LLM providers · Universal stack doctor · Parallel agent swarms
    <br />
    <br />
    <a href="#one-command-installation"><strong>Quick Install »</strong></a>
    &nbsp;·&nbsp;
    <a href="#usage">Usage</a>
    &nbsp;·&nbsp;
    <a href="#minidoctor-universal-stack-health">MiniDoctor</a>
    &nbsp;·&nbsp;
    <a href="#interactive-theme-studio">Theme Studio</a>
    &nbsp;·&nbsp;
    <a href="https://github.com/aswin402/minicode/issues/new?labels=bug">Report Bug</a>
    &nbsp;·&nbsp;
    <a href="https://github.com/aswin402/minicode/issues/new?labels=enhancement">Request Feature</a>
  </p>
</div>

<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#feature-highlights">Feature Highlights</a></li>
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#one-command-installation">Getting Started &amp; Installation</a>
      <ul>
        <li><a href="#linux--macos">Linux &amp; macOS (Single Command)</a></li>
        <li><a href="#windows-powershell">Windows PowerShell (Single Command)</a></li>
        <li><a href="#build-from-source-cargo">Build From Source (Cargo)</a></li>
        <li><a href="#first-run-setup">First-Run Setup</a></li>
      </ul>
    </li>
    <li>
      <a href="#usage">Usage</a>
      <ul>
        <li><a href="#interactive-tui--slash-commands">Interactive TUI &amp; Slash Commands</a></li>
        <li><a href="#headless-cli-ndjson--mcp-server">Headless CLI, NDJSON &amp; MCP Server</a></li>
        <li><a href="#minidoctor-universal-stack-health">MiniDoctor: Universal Stack Health</a></li>
        <li><a href="#interactive-theme-studio">Interactive 21-Tab Theme Studio</a></li>
      </ul>
    </li>
    <li><a href="#architecture--workspace-crates">Architecture &amp; Workspace Crates</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

---

## About The Project

**`minicode`** is a single-binary, pure-Rust AI coding agent that brings full autonomous engineering directly into your terminal without Electron bloat, Python virtualenvs, or OpenSSL system dependencies.

Instead of splitting your screen into rigid dashboard panes, `minicode` streams a clean, low-latency inline timeline (`ratatui`) for human pairing **and** switches seamlessly into machine-readable NDJSON (`--json-stream`) or Model Context Protocol (`minicode serve`) when driven by CI pipelines or external orchestrators.

### Feature Highlights

| Pillar | What Ships Out of the Box |
| :--- | :--- |
| **215 Built-In Tools** | Lazy-activated domain toolpacks covering AST code edits, ripgrep/Tree-sitter code graphs, git workflows, shell sandboxing, browser automation, memory, wiki, vault, and dev-server supervision. |
| **23 LLM Providers** | Anthropic, OpenRouter, Gemini, OpenAI, DeepSeek, Groq, z.ai, Kimi, HuggingFace, NVIDIA, OpenCode, Cerebras, Mistral, Together, MiniMax, Agnes, Nara, OFox, AIHubMix, plus local **Ollama**, **LM Studio**, **vLLM**, and **Localhost** — all over pure-Rust `rustls` TLS. |
| **MiniDoctor (`minidoctor`)** | Native Rust universal codebase health engine (**44 AST + heuristic rules** across React, Next.js, TypeScript/JS, Hono, Rust, Python, Flutter/Dart, Go, and universal security/quality). Produces a `0–100` health score, dead-code analysis, git-diff scans, SARIF/JSON exports, and `.minicode/minidoctor/NEEDEDFIX.md` for human + AI agent handoff. |
| **MiniPubs Swarm (14 Roles)** | Autonomous multi-agent waves (`Scout`, `Architect`, `Planner`, `Designer`, `Frontend`, `Backend`, `Coder`, `Vision`, `ContentWriter`, `Debugger`, `Reviewer`, `Security`, `Tester`, `Simplifier`) with isolated git worktrees and cooperative file leases. |
| **MiniBrowse (`minibrowse`)** | Ultra-low-RAM two-gear agentic browser: fast pure-Rust HTTP + DOM + Markdown extractor paired with CDP / WebKit live inspection and Flutter VM hot-reload diagnostics. |
| **MiniKit (13 Stacks + Blocks)** | One-command project scaffolding (`react-vite`, `react-vite-full`, `react-vite-gsap`, `next-template`, `hono-api`, `hono-full`, `express-api`, `fastapi`, `mern`, `pern`, `rust-cli`, `flutter-riverpod`, `static-website`), modular `MiniBlocks`, and multi-registry package search (`npm`, `PyPI`, `crates.io`, `pub.dev`). |
| **MiniPower & Safe Rollbacks** | Persistent `/goal` autonomous loops, background dev-server & port supervisor (`/tasks`), Linux Landlock + rlimit sandboxing, and atomic `/undo` workspace transaction snapshots. |
| **21-Tab `/theme` Studio** | Live TUI customization for palettes, thinking animations, todo/swarm/toast/tool/output/turn-summary cards, **10 Doctor styles × 6 live working animations**, and per-element Show/Hide toggles. |

### Built With

[![Rust][rust-badge]][rust-url]
[![Tokio][tokio-badge]][tokio-url]
[![Ratatui][ratatui-badge]][ratatui-url]
[![Tree-sitter][treesitter-badge]][treesitter-url]
[![Rustls][rustls-badge]][rustls-url]

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## One-Command Installation

Prebuilt release archives bundle **`minicode`**, **`minidoctor`**, and **`minibrowse`** together, verify **SHA-256 checksums** automatically, configure your `PATH`, and fall back gracefully to `cargo` if needed.

### Linux & macOS

Works on **Linux (`x86_64`, `aarch64`)**, **macOS (`Apple Silicon`, `Intel`)**, **WSL**, and **Git Bash**:

```bash
curl -fsSL https://raw.githubusercontent.com/aswin402/minicode/main/install.sh | bash
```

<details>
<summary>Installer options &amp; environment variables (Linux / macOS)</summary>

```bash
# Pin a specific version
MINICODE_VERSION=v0.5.86 curl -fsSL https://raw.githubusercontent.com/aswin402/minicode/main/install.sh | bash

# Custom install directory
MINICODE_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/aswin402/minicode/main/install.sh | bash

# Dry-run (inspect detected target & release URL without modifying files)
MINICODE_DRY_RUN=1 curl -fsSL https://raw.githubusercontent.com/aswin402/minicode/main/install.sh | bash

# Skip optional Obscura helper or shell rc PATH edits
SKIP_OBSCURA=1 MINICODE_NO_PATH=1 curl -fsSL https://raw.githubusercontent.com/aswin402/minicode/main/install.sh | bash
```

</details>

### Windows (PowerShell)

Run in **Windows Terminal** or **PowerShell 5.1 / 7+**:

```powershell
irm https://raw.githubusercontent.com/aswin402/minicode/main/install.ps1 | iex
```

<details>
<summary>Installer options &amp; environment variables (Windows)</summary>

```powershell
# Pin a specific version or custom directory
$env:MINICODE_VERSION = "v0.5.86"
$env:MINICODE_INSTALL_DIR = "$env:USERPROFILE\bin"
irm https://raw.githubusercontent.com/aswin402/minicode/main/install.ps1 | iex
```

</details>

### Build From Source (Cargo)

Requires Rust 1.80+ (`rustup`):

```bash
cargo install --git https://github.com/aswin402/minicode.git --locked minicode
```

Or clone and install the full workspace (`minicode`, `minidoctor`, `minibrowse`):

```bash
git clone https://github.com/aswin402/minicode.git
cd minicode
bash localupdate.sh --clean
```

### First-Run Setup

Verify your installation, connect an LLM provider, and launch:

```bash
minicode --version
minicode setup      # Interactive wizard for API keys, models & local Ollama/LM Studio
minicode            # Launch the interactive TUI in the current repository
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## Usage

### Interactive TUI & Slash Commands

Launch `minicode` inside any project directory to enter the streaming Ratatui timeline:

```bash
cd your-project
minicode
```

| Command | Action |
| :--- | :--- |
| `/doctor [flags \| fix]` | Run MiniDoctor health audit (inline animated card + `.minicode/minidoctor/NEEDEDFIX.md`) |
| `/doctor-style [name]` | Switch between 10 Doctor visual chrome styles (`aura`, `cyber`, `neon`, `kaomoji`, `ascii`, …) |
| `/doctor-anim [name]` | Switch between 6 live Doctor working animations (`pulse`, `radar`, `orbit`, `kaomoji`, `blocks`, `ascii`) |
| `/theme [preset]` | Open the 21-tab live TUI Theme & Style Studio |
| `/model` | Hot-swap active provider & model mid-session |
| `/goal [add \| done \| run]` | Drive persistent multi-step autonomous goal loops |
| `/todo` (`F8`) | Inspect and track structured session task checklists |
| `/diff [--cached]` | Inspect syntax-colored workspace git diffs inline |
| `/review` | Run multi-agent adversarial code review on pending changes |
| `/undo` | Roll back the latest atomic workspace transaction |
| `/blocks` / `/tasks` | Manage MiniBlocks and live background dev servers |
| `/resume [last \| id]` | Browse and resume persisted JSONL sessions |
| `/copy [all]` | Copy latest response or full transcript to OS clipboard |

### Headless CLI, NDJSON & MCP Server

Automate tasks in CI/CD, scripts, or orchestrator pipelines:

```bash
# One-shot autonomous task execution
minicode run "Fix the failing unit tests in src/auth and verify cargo test passes"

# Machine-readable NDJSON event streaming for scripts & sub-orchestrators
minicode --json-stream run "Audit API routes for missing auth guards"

# Expose minicode workspace tools as a Model Context Protocol (MCP) server over stdio
minicode serve --dir ./my-repo

# Scaffold a production-ready MiniKit stack
minicode stack list
minicode init my-api --stack hono-api

# Multi-ecosystem package lookup (npm, PyPI, crates.io, pub.dev)
minicode info axum --runtime cargo
```

### MiniDoctor: Universal Stack Health

Run **`minicode doctor`** (or the standalone **`minidoctor`** binary) on any repository for instant native-Rust static diagnostics:

```bash
# Full project audit + generate .minicode/minidoctor/NEEDEDFIX.md
minicode doctor

# Audit only git-changed files against main (great for PR CI gates)
minicode doctor --diff main --threshold 80

# Emit numeric health score (0-100) or SARIF for GitHub Code Scanning
minicode doctor --score
minicode doctor --format sarif > minidoctor.sarif

# Explain any specific rule ID
minicode doctor --explain rust/no-unwrap-prod
```

Every scan writes an actionable, severity-sorted remediation guide to **`.minicode/minidoctor/NEEDEDFIX.md`** so both humans and AI agents (including `minicode` itself via `/doctor fix`) share the exact same fix checklist.

### Interactive 21-Tab Theme Studio

Run **`/theme`** inside the TUI to customize every visual layer live with instant miniature previews:

- **Palettes & Motion**: Built-in themes (Aura Dark, Catppuccin, Tokyo Night, Dracula, Gruvbox, Nord, Solarized, …) + thinking animations.
- **Component Chrome**: Dedicated tabs for Todo Lists, Agent Swarms, Toasts, Activity Bar, Menus, Input Box, Status Bar, Context/KV Meter, Mode Badges, Tool Cards & Icons, Output Frames/Badges, Turn Summaries, and **Doctor Styles + Animations** (`[a]`, `[`, `]`).
- **21. Show / Hide**: Granular toggles for every status segment, hint, KV cache telemetry, and inline Doctor card detail drawer.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## Architecture & Workspace Crates

`minicode` is structured as a lean 3-crate Rust workspace with strict modularity and zero C OpenSSL dependencies:

```text
minicode/
├── src/                        # Core minicode binary (TUI + CLI + MCP server)
│   ├── main.rs                 # Interactive Ratatui TUI, headless --json-stream, CLI & MCP dispatch
│   ├── agent/                  # Autonomous loop, MiniPubs swarm (14 roles), verification & memory
│   ├── context/                # Tree-sitter AST repomap, petgraph code graph, token/KV optimizer
│   ├── llm/                    # 23-provider pure-Rust streaming HTTP/SSE client (rustls)
│   ├── tools/                  # 215 modular tools (fs, AST, git, shell, browser, doctor, MiniKit)
│   ├── sandbox/                # Landlock (Linux), rlimit budgets, git worktrees, rollback tx
│   ├── dev/                    # Background dev-server supervisor, port/RSS telemetry
│   └── ui/                     # Inline streaming Ratatui timeline, 21-tab /theme studio, modals
├── crates/
│   ├── minidoctor/             # Universal multi-stack native Rust codebase doctor (lib + CLI)
│   └── minibrowse/             # Ultra-low-RAM agentic browser, markdown extractor & Flutter inspector
├── .github/workflows/          # Cross-platform CI & 5-target release pipeline
├── install.sh                  # Single-command installer for Linux, macOS, WSL & Git Bash
└── install.ps1                 # Single-command installer for Windows PowerShell
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## Roadmap

- [x] Pure-Rust dual-mode TUI (`ratatui`) + headless `--json-stream` + MCP `serve`
- [x] 215 lazy-activated built-in tools & 23 cloud/local LLM providers over `rustls`
- [x] `MiniPubs` parallel specialist swarm (14 roles) with git worktrees & file leases
- [x] `MiniDoctor` native Rust universal codebase health engine + `.minicode/minidoctor/NEEDEDFIX.md`
- [x] `MiniBrowse` low-RAM two-gear browser & Flutter VM hot-reload inspector
- [x] 21-tab `/theme` customization studio (including 10 Doctor styles & 6 working animations)
- [x] Single-command cross-platform release installers (`Linux`, `macOS`, `Windows`)
- [ ] Homebrew tap (`brew install aswin402/tap/minicode`) & Winget / Scoop manifests
- [ ] Additional `MiniDoctor` AST packs (Java/Kotlin, C#, Swift, Elixir)
- [ ] Plugin SDK for community-authored MiniKit stacks and custom toolpacks

See the [open issues](https://github.com/aswin402/minicode/issues) for a full list of proposed features and known issues.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## Contributing

Contributions make the open-source community thrive — PRs, bug reports, new `MiniDoctor` rules, and `MiniKit` stacks are warmly welcome!

1. Fork the repository and create your feature branch (`git checkout -b feat/amazing-feature`).
2. Keep local builds laptop-friendly and enforce zero warnings:
   ```bash
   cargo fmt --check
   nice -n 19 cargo clippy -j 1 -- -D warnings
   nice -n 19 cargo test -j 1
   ```
3. Commit your changes (`git commit -m 'feat: add amazing feature'`).
4. Push your branch (`git push origin feat/amazing-feature`) and open a Pull Request.

See [AGENTS.md](AGENTS.md) for architecture conventions and invariants (no `.unwrap()`/`.expect()` in non-test code, `tracing` logging instead of `println!`, pure-Rust TLS).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## License

Dual-licensed under **MIT** or **Apache-2.0**, at your option. See `Cargo.toml` for metadata.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## Contact

**Aswin** — [@aswin402](https://github.com/aswin402)

Project Link: [https://github.com/aswin402/minicode](https://github.com/aswin402/minicode)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

---

## Acknowledgments

- [Ratatui](https://ratatui.rs) — Modern Rust terminal UI rendering
- [Tree-sitter](https://tree-sitter.github.io) — Incremental multi-language AST parsing
- [Tokio](https://tokio.rs) & [Rustls](https://github.com/rustls/rustls) — Pure-Rust async runtime & TLS
- [Best-README-Template](https://github.com/othneildrew/Best-README-Template) — Clean README layout inspiration

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
[release-shield]: https://img.shields.io/github/v/release/aswin402/minicode?style=for-the-badge&color=a277ff&labelColor=15141b
[release-url]: https://github.com/aswin402/minicode/releases
[ci-shield]: https://img.shields.io/github/actions/workflow/status/aswin402/minicode/ci.yml?branch=main&style=for-the-badge&label=CI&labelColor=15141b
[ci-url]: https://github.com/aswin402/minicode/actions/workflows/ci.yml
[rust-shield]: https://img.shields.io/badge/Rust-2021-61ffca?style=for-the-badge&logo=rust&logoColor=15141b&labelColor=15141b
[rust-url]: https://www.rust-lang.org/
[platforms-shield]: https://img.shields.io/badge/Platforms-Linux%20%7C%20macOS%20%7C%20Windows-82e2ff?style=for-the-badge&labelColor=15141b
[license-shield]: https://img.shields.io/badge/License-MIT%20%7C%20Apache--2.0-f694ff?style=for-the-badge&labelColor=15141b
[license-url]: https://github.com/aswin402/minicode/blob/main/Cargo.toml
[stars-shield]: https://img.shields.io/github/stars/aswin402/minicode?style=for-the-badge&color=ffca85&labelColor=15141b
[stars-url]: https://github.com/aswin402/minicode/stargazers
[issues-shield]: https://img.shields.io/github/issues/aswin402/minicode?style=for-the-badge&color=ff6767&labelColor=15141b
[issues-url]: https://github.com/aswin402/minicode/issues

[rust-badge]: https://img.shields.io/badge/Rust-15141b?style=for-the-badge&logo=rust&logoColor=61ffca
[tokio-badge]: https://img.shields.io/badge/Tokio-15141b?style=for-the-badge&logo=rust&logoColor=82e2ff
[tokio-url]: https://tokio.rs/
[ratatui-badge]: https://img.shields.io/badge/Ratatui_TUI-15141b?style=for-the-badge&logo=gnometerminal&logoColor=a277ff
[ratatui-url]: https://ratatui.rs/
[treesitter-badge]: https://img.shields.io/badge/Tree--sitter_AST-15141b?style=for-the-badge&logo=treehouse&logoColor=61ffca
[treesitter-url]: https://tree-sitter.github.io/
[rustls-badge]: https://img.shields.io/badge/Pure_Rust_TLS-15141b?style=for-the-badge&logo=letsencrypt&logoColor=f694ff
[rustls-url]: https://github.com/rustls/rustls
