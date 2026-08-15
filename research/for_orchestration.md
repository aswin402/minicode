# Agent Orchestration, Multi-Agent Systems, Task Management & Planning: Deep Technical Research

**Target Project:** `minicode` — Fast, Minimalist Pure-Rust TUI + CLI AI Coding Agent  
**Date:** August 2026  
**Document Status:** Complete Architecture & Research Reference  
**Output Path:** `research/for_orchestration.md`

---

## Executive Summary

As AI coding agents transition from single-turn chat completion loops to autonomous multi-step software engineers, orchestration and runtime management become the core determinants of reliability, token efficiency, and execution speed.

This research document analyzes **10 state-of-the-art systems and frameworks** across agent orchestration, multi-agent collaboration, task decomposition, and execution environments:

1. [**Cowork Forge**](https://github.com/sopaco/cowork-forge) (`sopaco/cowork-forge`) — Full-stack Rust AI development team with Actor-Critic validation and pipeline stages.
2. [**Claude Task Master**](https://github.com/eyaltoledano/claude-task-master) (`eyaltoledano/claude-task-master`) — PRD decomposition, topological task DAGs, and complexity scoring engine.
3. [**LangGraph**](https://github.com/langchain-ai/langgraph) (`langchain-ai/langgraph`) — Low-level stateful graph orchestration, super-step checkpointing, and time-travel debugging.
4. [**Herdr**](https://github.com/herdrdev/herdr) (`herdrdev/herdr`) — Pure Rust terminal workspace multiplexer and background daemon runtime for coding agents.
5. [**RTK (Rust Token Killer)**](https://github.com/rtk-ai/rtk) (`rtk-ai/rtk`) — High-performance Rust CLI proxy cutting 60–90% of bash output token waste.
6. [**Daytona**](https://github.com/daytonaio/daytona) (`daytonaio/daytona`) — Elastic, ephemeral sandbox infrastructure for secure code execution.
7. [**Dify**](https://github.com/langgenius/dify) (`langgenius/dify` & `dify-agent-runtime`) — Multi-agent workflow visual DAG engine and Landlock-isolated Go runtime.
8. [**Astrid Runtime**](https://github.com/astrid-runtime/astrid) (`astrid-runtime/astrid`) — Capability-secure microkernel operating system for AI agents in Rust 2024.
9. [**Awesome Claude Code Subagents**](https://github.com/VoltAgent/awesome-claude-code-subagents) (`VoltAgent/awesome-claude-code-subagents`) — Catalog of 158 specialized subagent definitions, meta-orchestrators, and tool isolation patterns.
10. [**ECC (Enhanced Coding Companion / ECC 2.0)**](https://github.com/affaan-m/ecc) (`affaan-m/ecc`) — Agentic IDE control plane, worktree orchestration, and Rust TUI dashboard (`ecc-tui`).

---

## Comparative Architecture Matrix

| Project | Primary Domain | Stack / Language | Orchestration Model | State & Memory Model | Sandboxing / Isolation | Relevance to `minicode` |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Cowork Forge** | Virtual AI Dev Team | Rust 2024 (`adk-rust`, `tokio`) | Staged Pipeline + Actor-Critic Loops | File Artifacts + Iteration Store (JSON) | ACP (Agent Client Protocol) / Subprocess | **High** (Rust native, Actor-Critic stage executor) |
| **Claude Task Master** | Task & PRD Decomposition | TypeScript / Node / MCP | Topological DAG + Recursive Breakdown | JSON File Tasks + Tag Partitioning | Process boundary / MCP | **High** (Task graph resolution, complexity scoring) |
| **LangGraph** | Stateful Agent Graphs | Python / TypeScript | StateGraph (Cyclic DAGs, Super-steps) | Channel Reducers + SQLite/Postgres Checkpointing | Application-level state boundaries | **Critical** (Durable execution, graph state machines) |
| **Herdr** | Agent Workspace Multiplexer | Pure Rust (`ratatui`, `portable-pty`, `interprocess`) | Daemon-Client IPC (`/tmp/herdr.sock`) | Persistent Session Snapshots (Bincode/JSON) | Headless PTY per agent | **Critical** (TUI multiplexing, agent state detection) |
| **RTK AI** | Token Reduction Proxy | Pure Rust (`rusqlite`, `ignore`, `regex`, `toml`) | Command Interception & AST Filtering | SQLite Analytics & Local Cache | Stateless CLI Interceptor | **Critical** (60–90% token reduction in tool execution) |
| **Daytona** | Agent Sandbox Infra | Go / TypeScript / Docker | Interface/Control/Compute Planes | Ephemeral Disk Snapshots + Volumes | Docker / MicroVM / OCI Sandboxes (<90ms) | **Medium** (Remote sandbox architecture reference) |
| **Dify** | Multi-Agent Visual Workflow | Python / Go (`landlock`) / Next.js | Visual Workflow DAG + Role Coordination | Vector DB + SQLite/Postgres + Redis | Linux Landlock + PTY Sanitization | **High** (Shared Linux Landlock security model) |
| **Astrid** | Capability-Secure Agent OS | Pure Rust 2024 (`wasmtime`, `blake3`, `ed25519`) | Microkernel Event Bus (`astrid-events`) | Cryptographic Grants + KV Store | WASM Capsules (Zero ambient authority) | **High** (Microkernel design, explicit capability tokens) |
| **Awesome Subagents** | Subagent Prompt Catalog | YAML Frontmatter + Markdown | Hierarchical / Meta-Orchestrator Routing | Shared Markdown Context / Transcripts | Tool permission whitelisting per agent | **High** (Subagent role taxonomy & schema design) |
| **ECC (ECC 2.0)** | Agentic IDE Control Plane | Rust (`ecc-tui`, `ratatui 0.30`, `git2`, `tokio`) | Worktree Orchestration + HUD Statusline | SQLite State Store + Git Worktrees | Git Worktree Branch Isolation | **High** (TUI architecture, worktree-native sessions) |

---

## Detailed Project Deep Dives

### 1. Cowork Forge (`sopaco/cowork-forge`)
- **Repository:** [https://github.com/sopaco/cowork-forge](https://github.com/sopaco/cowork-forge)
- **What It Does:** An end-to-end AI software development system written in Rust that automates the entire lifecycle from requirements analysis to production code delivery by simulating a complete virtual engineering team.
- **Key Technical Highlights:**
  - **Pipeline Stages:** Implements a strict yet configurable sequence of engineering stages: `Idea` $\rightarrow$ `PRD` $\rightarrow$ `Design` $\rightarrow$ `Plan` $\rightarrow$ `Coding` $\rightarrow$ `Check` $\rightarrow$ `Delivery`.
  - **Actor-Critic Dual-Agent Pattern:** Each critical stage pairs a specialized *Actor* (e.g., `prd_actor`, `design_actor`, `coding_actor`) that generates draft artifacts with a dedicated *Critic* (e.g., `prd_critic`, `design_critic`, `coding_critic`) that verifies specifications, enforces quality rules, and signals iterative refinement loops before proceeding.
  - **Configuration-Driven Dynamic Stage Registry:** Defines flows and agent roles using declarative JSON schemas (`default_configs/flows/*.json` and `default_configs/agents/*.json`), instantiated at runtime via `agent_factory.rs`.
  - **ACP & MCP Protocol Support:** Implements the Agent Client Protocol (`agent-client-protocol = "0.9"`) to delegate heavy coding tasks to external agent runtimes while keeping orchestration inside Rust.
  - **Persistent Iteration Store:** Stores intermediate snapshots, user feedback history, and memory state across iterations (`persistence/iteration_store.rs`).
- **Inspiration for `minicode`:**
  - **Actor-Critic Code Modification Loop:** Integrate an internal Critic pass for code edits. When `minicode` generates a diff or code change, an autonomous Critic evaluation step can verify AST correctness, check tests (`cargo test` / `pytest`), and inspect tree-sitter syntax graphs before presenting changes to the user.
  - **Stage-Aware Execution Context:** Model multi-step CLI commands as explicit stages (`Analyze` $\rightarrow$ `Plan` $\rightarrow$ `Execute` $\rightarrow$ `Verify`) where context tokens are pruned between stages to keep prompt overhead low.
- **Rust Crates / Dependencies Worth Noting:**
  - `adk-rust`, `adk-core`, `adk-agent`, `adk-model` (Rust Agent Development Kit)
  - `agent-client-protocol` (Standardized external agent integration)
  - `dialoguer = "0.12"` and `console = "0.16"` (Interactive terminal prompts)
  - `thiserror = "2"`, `ignore = "0.4"`

---

### 2. Claude Task Master (`eyaltoledano/claude-task-master`)
- **Repository:** [https://github.com/eyaltoledano/claude-task-master](https://github.com/eyaltoledano/claude-task-master)
- **What It Does:** A structured task management and PRD decomposition engine for AI coding workflows that turns ambiguous software specs into atomic, dependency-managed task DAGs.
- **Key Technical Highlights:**
  - **Recursive Task Expansion:** Deconstructs high-level features (`parse-prd`) into hierarchical tasks and subtasks (`expand-task`, `expand-all`), ensuring each leaf task is small enough for an LLM context window.
  - **Task Complexity Scoring:** Features `analyze-task-complexity` and `complexity-report` heuristics that rate tasks (1–10 scale) based on scope, file dependencies, and risk. High-complexity tasks are automatically flagged for further decomposition before execution.
  - **Topological Dependency Graph Resolution:** Functions `validate-dependencies` and `fix-dependencies` detect circular dependencies, validate topological order, and surface the next actionable unblocked task (`next-task`).
  - **Tag-Based Worktree/Branch Isolation:** Supports `create-tag-from-branch` and tag partitioning to maintain distinct task graphs across Git branches and feature tracks.
- **Inspiration for `minicode`:**
  - **`petgraph`-Powered Task Engine:** `minicode` already includes `petgraph = "0.6"`. We can build a native Rust Task DAG module that stores tasks as graph nodes and dependencies as directed edges.
  - **Autonomous Next-Task Queue:** Implement a `/task` command in `minicode` that parses a `TODO.md` or PRD into a topological queue, executes unblocked tasks sequentially, and updates status upon test completion.
  - **Pre-Execution Complexity Gate:** Compute a complexity score before running code generators; if the predicted token footprint or file count exceeds safe limits, prompt `minicode` to decompose the task first.
- **Rust Crates / Dependencies Worth Noting:**
  - `petgraph = "0.6"` (Already in `minicode` — ideal for DAG cycle detection and topological sorting via `petgraph::algo::toposort`)
  - `schemars = "1.0"` (JSON Schema generation for task payloads)

---

### 3. LangGraph (`langchain-ai/langgraph`)
- **Repository:** [https://github.com/langchain-ai/langgraph](https://github.com/langchain-ai/langgraph)
- **What It Does:** A low-level, stateful orchestration framework for building robust, cyclic, and long-running multi-agent workflows with durable execution.
- **Key Technical Highlights:**
  - **StateGraph Architecture:** Models workflows as state machines where nodes are computation units (agent steps, tool calls) and edges are transitions (conditional branching or cyclical feedback loops).
  - **Channel Reducers & State Delta Management:** State updates occur through explicit reducer functions (e.g., append-only message logs, dictionary merges), preventing race conditions and unintentional state mutations.
  - **Super-Step Durable Execution & Checkpointing:** State is snapshotted to a persistent backend (`checkpoint-sqlite`, `checkpoint-postgres`) after every super-step. If a network call fails or the process terminates, the agent resumes from the exact last super-step.
  - **Time-Travel & State Forking:** Because every checkpoint is immutable, developers can inspect state history, rewind execution to step $N$, modify variables, and fork a new execution trajectory.
  - **Human-in-the-Loop Interrupts:** Native `interrupt()` semantics allow pausing the graph before sensitive operations (e.g., destructive bash commands or database migrations) and resuming upon user approval.
- **Inspiration for `minicode`:**
  - **Pure-Rust State Machine for Agent Turns:** Implement a `StateGraph<S>` in Rust where each node is an async function `async fn(State) -> Result<StateDelta>`.
  - **SQLite Checkpointing for Session Resume:** Serialize session state after each tool call to SQLite or JSONL. Users can crash, exit, or run `/rewind` to jump back to any previous tool call state.
  - **Explicit Approval Interrupters:** Model user approvals for shell execution (`run_command`) and file edits as state interrupts in the execution loop.
- **Rust Crates / Dependencies Worth Noting:**
  - `rusqlite = "0.31"` or `sqlx` (Fast, zero-overhead checkpoint persistence)
  - `tokio-stream = "0.1"` (Already in `minicode` — stream state deltas and events)
  - `async-trait = "0.1"` (Already in `minicode` — dynamic node handler dispatch)

---

### 4. Herdr (`herdrdev/herdr`)
- **Repository:** [https://github.com/herdrdev/herdr](https://github.com/herdrdev/herdr)
- **What It Does:** A pure-Rust terminal workspace multiplexer and background daemon designed specifically for AI coding agents to run uninterrupted across multiple sessions and worktrees.
- **Key Technical Highlights:**
  - **Client-Server Daemon Architecture:** A background server (`herdr-server`) manages headless PTY sessions over local Unix domain sockets / named pipes (`interprocess = "2.4"`). The terminal UI client connects, disconnects, or reconnects seamlessly without disrupting agent processes.
  - **Agent State Detection Engine:** Monitors raw terminal escape sequences, OSC codes, and output patterns across 20+ AI agent runtimes (Claude, Cursor, Copilot, Cline, Antigravity, OpenCode, Devin) using declarative TOML rules (`detect/manifests/*.toml`).
  - **Tri-State Activity Classifier:** Automatically classifies each agent pane into `Working` (spinning/generating), `Blocked` (awaiting user input/approval), or `Idle` (completed turn).
  - **TUI Multiplexer on Ratatui:** Implements high-performance terminal rendering, scrollback buffers, tab surfaces, and split panes using `ratatui = "0.30"` and `crossterm = "0.29"`.
  - **Session Snapshot & Handoff Persistence:** Serializes running pane layouts, environment states, and agent metadata to disk via `bincode = "2"` and JSON.
- **Inspiration for `minicode`:**
  - **Background Agent Daemon / Detached Mode:** Allow `minicode` to run in daemon mode (`minicode daemon` or `--detach`), letting the user close their laptop or switch terminals while `minicode` executes long-running test suites or multi-file refactors in a background PTY.
  - **Agent State Signal in TUI Statusline:** Port Herdr's lightweight output analyzer to `minicode`'s TUI statusline to display exact subagent states (`Working: Running Cargo Check`, `Blocked: Waiting for User Approval`, `Idle`).
  - **Native PTY Process Isolation:** Use `portable-pty` for running child shell commands with true ANSI terminal emulation and raw input/output streaming.
- **Rust Crates / Dependencies Worth Noting:**
  - `portable-pty = "=0.9.0"` (Cross-platform PTY creation and process management)
  - `interprocess = "2.4.2"` (High-performance IPC Unix sockets / Windows named pipes)
  - `ratatui = "0.30"` & `crossterm = "0.29"` (State-of-the-art terminal rendering)
  - `bincode = "2"` (Ultra-fast binary serialization of session state)

---

### 5. RTK — Rust Token Killer (`rtk-ai/rtk`)
- **Repository:** [https://github.com/rtk-ai/rtk](https://github.com/rtk-ai/rtk)
- **What It Does:** A high-performance Rust CLI proxy that intercepts and compresses shell command outputs before they enter LLM context windows, eliminating 60–90% of token consumption.
- **Key Technical Highlights:**
  - **Declarative TOML Output Filters:** Ships with over 60 pre-built TOML filters (`src/filters/*.toml`) for standard developer tools (`cargo`, `pytest`, `npm`, `tsc`, `git diff`, `docker`, `terraform`, `gcloud`).
  - **AST-Aware Code Pruners:** Strips non-essential comments, whitespace, and repetitive verbose boilerplates from source files and test outputs using language-specific strategies (`src/cmds/rust/cargo_cmd.rs`, `src/cmds/python/pytest_cmd.rs`).
  - **Intelligent Error Extractor:** When builds fail, RTK discards thousands of lines of successful compilation noise and isolates the root compiler error, file path, line number, and stack trace.
  - **SQLite Analytics & Token Savings Ledger:** Records token counts before and after filtering into local SQLite (`rusqlite`), computing real-time dollar savings and efficiency metrics.
  - **Sub-10ms Overhead:** Compiled as a single optimized Rust release binary with LTO and zero runtime dependencies.
- **Inspiration for `minicode`:**
  - **Native Tool Output Compactor for `minicode`:** `minicode` frequently executes `run_command`, `git diff`, and `cargo check`. Integrating RTK-style output filters directly into `src/tools/` will prevent context window blowout and drastically cut LLM API costs.
  - **Compiler Error Distiller:** When `minicode` runs `cargo check` or `pytest`, automatically compress the output to only extract the failed assertion or compiler diagnostic before feeding it back into the model prompt.
  - **Session Token Analytics Display:** Show a compact metric in `minicode`'s TUI footer: `Tokens Saved: 42.5k (78%) | Cost Avoided: $0.13`.
- **Rust Crates / Dependencies Worth Noting:**
  - `rusqlite = "0.31"` (Local analytics and token caching)
  - `ignore = "0.4"` (Already in `minicode` — directory scanning respecting `.gitignore`)
  - `regex = "1.10"` (Already in `minicode` — high-speed regex parsing of compiler outputs)
  - `toml = "0.8"` (Already in `minicode` — user-customizable tool filter rules)

---

### 6. Daytona (`daytonaio/daytona`)
- **Repository:** [https://github.com/daytonaio/daytona](https://github.com/daytonaio/daytona)
- **What It Does:** A secure, elastic runtime and infrastructure orchestrator for executing AI-generated code inside disposable, sub-90ms sandboxes with dedicated compute, filesystems, and network isolation.
- **Key Technical Highlights:**
  - **Three-Plane Architecture:**
    - **Interface Plane:** Multi-language SDKs (Python, TS, Go, Java, Ruby), CLI, and REST APIs for seamless client integration.
    - **Control Plane:** Orchestrates sandbox lifecycles, handles authentication, API keys, and routes work requests.
    - **Compute Plane:** Spawns and manages isolated execution environments on Docker, OCI containers, or microVMs.
  - **Process Execution & PTY Streaming:** Provides real-time bidirectional streaming of stdout/stderr, pseudo-terminal multiplexing, and file synchronization.
  - **LSP (Language Server Protocol) Integration:** Sandboxes expose remote LSP endpoints for symbol resolution, diagnostics, and code navigation directly inside the execution environment.
- **Inspiration for `minicode`:**
  - **Pluggable Execution Provider Trait:** Abstract `minicode`'s tool runner behind an `ExecutionBackend` trait in Rust:
    ```rust
    #[async_trait]
    pub trait ExecutionBackend: Send + Sync {
        async fn run_command(&self, cmd: &str, cwd: &Path) -> Result<CommandOutput>;
        async fn read_file(&self, path: &Path) -> Result<Vec<u8>>;
        async fn write_file(&self, path: &Path, content: &[u8]) -> Result<()>;
    }
    ```
    This enables `minicode` to execute commands locally on the host via `LocalProcessBackend` (protected by Landlock) or remotely inside a containerized sandbox via `RemoteSandboxBackend`.
- **Rust Crates / Dependencies Worth Noting:**
  - `bollard = "0.16"` (Pure Rust Docker API client if managing local containers)
  - `tonic` / `prost` (gRPC streaming for high-performance agent runtime communication)

---

### 7. Dify (`langgenius/dify` & `dify-agent-runtime`)
- **Repository:** [https://github.com/langgenius/dify](https://github.com/langgenius/dify)
- **What It Does:** An open-source multi-agent application development platform combining visual DAG workflow design with a dedicated sandboxed agent runtime (`dify-agent-runtime`).
- **Key Technical Highlights:**
  - **Linux Landlock Sandboxing:** `dify-agent-runtime` uses Linux Landlock (`internal/landlock/landlock_linux.go`) to restrict child agent processes to specific workspace paths, prohibiting unauthorized read/write access to sensitive host directories.
  - **Process Control & Sanitization:** Features `shellctl`, `runner`, `runner-exit`, and `sanitize-pty` to safely control process lifecycles, sanitize VT100 control characters, and prevent runaway background commands.
  - **Visual & Programmatic Workflow DAGs:** Allows composing complex multi-agent chains with branching nodes, evaluator nodes, code execution sandboxes, and LLM routers.
- **Inspiration for `minicode`:**
  - **Zero-Trust Tool Isolation with Landlock:** `minicode` already includes `landlock = "0.4"` in `Cargo.toml`. Dify's implementation confirms that Landlock is the gold standard for native Linux agent security. `minicode` can enforce strict read-only rules across root filesystems while granting read-write authority strictly to the target repository directory.
  - **ANSI / PTY Output Sanitizer:** Clean and strip dangerous control characters (e.g., cursor repositioning, clear screen codes) from tool output before appending to session logs.
- **Rust Crates / Dependencies Worth Noting:**
  - `landlock = "0.4"` (Already in `minicode` — Linux kernel filesystem sandboxing)
  - `strip-ansi-escapes = "0.2"` (Fast stripping of terminal formatting for clean LLM prompt context)

---

### 8. Astrid Runtime (`astrid-runtime/astrid`)
- **Repository:** [https://github.com/astrid-runtime/astrid](https://github.com/astrid-runtime/astrid)
- **What It Does:** A capability-secure microkernel operating system written in pure Rust (2024 edition) that executes AI agent capabilities inside isolated WebAssembly capsules with zero ambient authority.
- **Key Technical Highlights:**
  - **Cryptographic Capability Grants:** Every tool call, filesystem path access, and network host connection requires an explicit, signed Ed25519 cryptographic token (`astrid-capabilities`, `ed25519-dalek`, `blake3`). No ambient permissions exist.
  - **Minimalist Microkernel (`astrid-kernel`):** The kernel contains zero LLM prompt logic or business code. It is exclusively responsible for routing events across the event bus (`astrid-events`), validating cryptographic capabilities, enforcing quotas, and maintaining an immutable audit log (`astrid-audit`).
  - **WASM Capsule Isolation (`astrid-capsule`):** Tools and agent plugins run inside Wasmtime sandboxes with strictly typed WebAssembly Interface Type (WIT) boundaries.
  - **Unified Virtual Filesystem (`astrid-vfs`):** Enforces capability-scoped file access policies before requests reach the host OS.
- **Inspiration for `minicode`:**
  - **Capability-Based Tool Security Model:** Instead of giving the agent unconditional access to all tools, introduce explicit capability grants in `minicode`. For instance, an agent role initialized with `Capability::ReadOnly` can invoke `read_file`, `grep_search`, and `list_dir`, but is rejected at the kernel level if it attempts `write_file` or `run_command`.
  - **Decoupled Event Bus:** Structure `minicode`'s internal communication around an async event bus (`EventBus<AgentEvent>`) where the UI, subagents, tools, and logger subscribe to decoupled broadcast channels (`tokio::sync::broadcast`).
  - **Tamper-Evident Audit Logging:** Hash all tool executions and responses using Blake3 (`blake3`) in `minicode`'s session history for verifiable session reproduction.
- **Rust Crates / Dependencies Worth Noting:**
  - `blake3 = "1.5"` (Cryptographically secure, SIMD-accelerated hashing)
  - `ed25519-dalek = "2.1"` (Fast Ed25519 signature verification for capability tokens)
  - `wasmtime = "25.0"` (WASM execution engine for untrusted agent plugins)
  - `async-trait = "0.1"` (Already in `minicode`)

---

### 9. Awesome Claude Code Subagents (`VoltAgent/awesome-claude-code-subagents`)
- **Repository:** [https://github.com/VoltAgent/awesome-claude-code-subagents](https://github.com/VoltAgent/awesome-claude-code-subagents)
- **What It Does:** The definitive collection of 158 specialized subagent definitions, prompt blueprints, and meta-orchestration patterns categorized for AI coding workflows.
- **Key Technical Highlights:**
  - **Standardized Agent Frontmatter Schema:** Each subagent is defined using clean YAML frontmatter specifying:
    - `name`: Unique agent identifier (e.g., `rust-engineer`, `context-manager`, `error-coordinator`).
    - `description`: Trigger conditions and delegation rules.
    - `tools`: Explicit tool whitelist (e.g., `Read, Write, Edit, Glob, Grep` — omitting `Bash` for safety).
    - `model`: Optimal model tier (e.g., `haiku` for fast parsing, `sonnet` for deep reasoning).
  - **Meta-Orchestration Subagents:** Dedicated coordinators that operate on multi-agent workflows:
    - `workflow-orchestrator`: Designs state machine definitions and rollback logic.
    - `context-manager`: Manages shared state files and directory structures across agents.
    - `error-coordinator`: Mines error logs for recurring patterns and cascade prevention.
    - `multi-agent-coordinator`: Plans concurrent subagent sequencing and handoffs.
  - **Tool Least-Privilege Design:** Language specialists and reviewers are restricted to read-only tools, preventing accidental destructive operations.
- **Inspiration for `minicode`:**
  - **Declarative Subagent Markdown Spec in `minicode`:** Allow users to define custom subagents inside `.minicode/agents/*.md` using YAML frontmatter + prompt guidelines.
  - **Model Tier Routing per Subagent:** When delegating tasks, route quick summaries, file discovery, and lint checks to lightweight models (e.g., Claude Haiku / GPT-4o-mini), reserving flagship reasoning models for complex architectural modifications.
  - **Built-in Specialized Roles:** Ship `minicode` with pre-configured internal subagents: `@planner`, `@reviewer`, `@security`, `@debugger`, and `@orchestrator`.
- **Rust Crates / Dependencies Worth Noting:**
  - `serde_yaml = "0.9"` (Fast YAML frontmatter deserialization from Markdown files)
  - `pulldown-cmark = "0.12"` (Already in `minicode` — parse and extract sections from agent markdown specs)

---

### 10. ECC — Enhanced Coding Companion / ECC 2.0 (`affaan-m/ecc`)
- **Repository:** [https://github.com/affaan-m/ecc](https://github.com/affaan-m/ecc)
- **What It Does:** An agentic developer platform and IDE control plane featuring a Rust TUI dashboard (`ecc-tui`), Git worktree orchestration, and an extensive suite of commands, hooks, and multi-agent workflows.
- **Key Technical Highlights:**
  - **`ecc-tui` in Pure Rust:** Built with `ratatui = "0.30"`, `crossterm = "0.29"`, `tokio = "1"`, `rusqlite = "0.40"`, and `git2 = "0.21"`.
  - **Worktree-Native Parallel Agent Sessions:** Enables agents to run simultaneously on isolated Git worktrees (`git2`), avoiding branch conflicts and workspace pollution during concurrent feature implementation.
  - **Unified HUD Statusline & Telemetry:** Exposes active agent status, context token consumption, cost metrics, unblocked todo queues, and risk assessment scores in an always-visible terminal status bar.
  - **Session Persistence & Memory Schemas:** Formalizes JSON schemas for session provenance (`provenance.schema.json`), memory persistence (`memory.schema.json`), and package management.
  - **Standardized Slash Commands:** Features pre-engineered command templates (`/plan-prd`, `/save-session`, `/rust-review`, `/test-coverage`).
- **Inspiration for `minicode`:**
  - **Worktree Isolation for Agent Tasks:** Integrate Git worktree management directly into `minicode`. When executing a multi-step refactor or exploratory branch, `minicode` can automatically create a temporary worktree, execute changes, verify tests, and present a clean diff for review.
  - **HUD Statusline Widget:** Implement an always-visible Ratatui HUD widget at the top or bottom of `minicode` showing context window usage, active subagent name, current stage, and unblocked task counts.
  - **Structured Memory Hooks:** Store project rules, architectural decisions, and error learnings in `.minicode/memory.json` to persist context across separate sessions.
- **Rust Crates / Dependencies Worth Noting:**
  - `git2 = "0.21"` (Native libgit2 bindings for high-speed branch and worktree manipulation)
  - `ratatui = "0.29"` / `0.30` (Already in `minicode`)
  - `rusqlite = "0.31"` (Local state store for session memory)
  - `cron = "0.17"` (Background recurring health checks and agent triggers)

---

## Top 5 Actionable Ideas for `minicode`

Based on this deep technical research, here are the **top 5 high-impact, Rust-native orchestration and multi-agent capabilities** recommended for direct implementation in `minicode`:

```
               ┌─────────────────────────────────────────────────────────────┐
               │             MINICODE ORCHESTRATION ARCHITECTURE             │
               └──────────────────────────────┬──────────────────────────────┘
                                              │
         ┌────────────────────────────────────┼────────────────────────────────────┐
         │                                    │                                    │
         ▼                                    ▼                                    ▼
┌──────────────────┐               ┌──────────────────┐               ┌──────────────────┐
│  1. Petgraph DAG │               │  2. Actor-Critic │               │  3. RTK Output   │
│   Task Engine    │               │ Verification Loop│               │ Token Compactor  │
│ (Claude Task/LG) │               │  (Cowork Forge)  │               │   (RTK Engine)   │
└────────┬─────────┘               └────────┬─────────┘               └────────┬─────────┘
         │                                  │                                  │
         └──────────────────────────────────┼──────────────────────────────────┘
                                            │
         ┌──────────────────────────────────┴──────────────────────────────────┐
         │                                                                     │
         ▼                                                                     ▼
┌──────────────────────────────────┐               ┌──────────────────────────────────┐
│   4. Background Agent Daemon     │               │   5. Capability-Gated Subagents  │
│       & PTY Multiplexer          │               │      with Landlock Sandboxing    │
│       (Herdr Architecture)       │               │      (Astrid + Dify + Claude)    │
└──────────────────────────────────┘               └──────────────────────────────────┘
```

---

### Idea 1: `petgraph`-Powered StateGraph & Topological Task Engine
- **Source Inspiration:** LangGraph (StateGraph) + Claude Task Master (PRD Decomposition & DAG).
- **Core Concept:** Replace linear tool execution with a directed graph state machine powered by `minicode`'s existing `petgraph = "0.6"` crate.
- **How It Works:**
  1. A user task or PRD is parsed into atomic tasks represented as graph nodes `TaskNode { id, description, status, complexity }`.
  2. Dependencies form directed edges. `petgraph::algo::toposort` and `petgraph::algo::is_cyclic_directed` validate the DAG and determine execution order.
  3. The orchestration engine executes all unblocked tasks concurrently (or sequentially), streaming state updates to the TUI.
  4. State is checkpointed to SQLite or JSONL after every node execution, enabling instant session pause, replay, and resume.
- **Why It Matters for `minicode`:** Transforms `minicode` from a reactive single-turn assistant into an autonomous project execution engine capable of planning and delivering multi-file, 20-step software features without losing track of state.

---

### Idea 2: Actor-Critic Dual-Agent Code Verification Loop
- **Source Inspiration:** Cowork Forge (`prd_actor`/`critic`, `coding_actor`/`critic`).
- **Core Concept:** Split code modification into a generative **Actor phase** and an adversarial **Critic verification phase** before presenting diffs to the user.
- **How It Works:**
  1. **Actor Phase:** The primary agent proposes a solution, edits files, and prepares a patch.
  2. **Critic Phase:** A specialized critic agent (or evaluation prompt) receives the diff along with tree-sitter AST diagnostics, linter output, and test execution results (`cargo check` / `cargo test`).
  3. If the critic detects broken invariants, compiler errors, or missing edge cases, it generates a structured `CriticFeedback` payload and triggers an internal iteration loop without bothering the user.
  4. Once the critic approves (or max iterations reached), the verified patch and execution summary are rendered cleanly in the Ratatui TUI.
- **Why It Matters for `minicode`:** Drastically elevates first-pass code accuracy, eliminates trivial syntax/type errors, and ensures code is self-tested before human review.

---

### Idea 3: RTK-Style Tool Output Compactor & Token Filter Engine
- **Source Inspiration:** RTK (Rust Token Killer).
- **Core Concept:** Intercept all CLI tool outputs (`run_command`, `cargo`, `git diff`, `grep`, `pytest`) and compress them using declarative regex/AST filters before injecting into the LLM context.
- **How It Works:**
  1. Define a native `OutputFilter` trait in `src/tools/filter.rs`.
  2. When a tool finishes execution:
     - For successful builds: Strip redundant compiler progress banners, leaving only summary status.
     - For failed builds: Discard successful compilation lines and extract only compiler errors, warning locations, and panic stack traces.
     - For file searches: Format results into compact tree representations instead of verbose path lists.
  3. Track total raw bytes vs compressed bytes saved in session metrics and display live savings in the TUI footer.
- **Why It Matters for `minicode`:** Reduces context window token consumption by **60% to 90%**, prevents context truncation during long coding sessions, and significantly reduces LLM inference costs.

---

### Idea 4: Background Daemon & Detached PTY Multiplexer
- **Source Inspiration:** Herdr (`portable-pty`, `interprocess`, daemon architecture).
- **Core Concept:** Decouple agent execution into a lightweight background daemon (`minicode-daemon`), allowing long-running tasks to continue while the user disconnects or switches views.
- **How It Works:**
  1. Run agent turns and child shell processes inside headless PTYs managed via `portable-pty`.
  2. Establish a local IPC socket (`interprocess`) between the daemon and the Ratatui TUI client.
  3. If the terminal closes, the agent continues running in the daemon. When the user relaunches `minicode`, it reconnects to the active socket and rehydrates the event stream.
  4. Include an activity state detector classifying background agent panes as `Working`, `Blocked`, or `Idle`.
- **Why It Matters for `minicode`:** Enables seamless execution of time-intensive workflows (e.g., large test suites, full repository indexing, end-to-end refactoring) without freezing the terminal or risking aborted processes upon terminal exit.

---

### Idea 5: Capability-Gated Subagent Registry with Landlock Sandboxing
- **Source Inspiration:** Astrid Runtime (Capability Security) + Awesome Claude Subagents (Schema) + Dify (Landlock).
- **Core Concept:** Implement a declarative subagent registry where each subagent is granted strict, capability-checked tool permissions enforced at compile-time and backed by Linux Landlock at runtime.
- **How It Works:**
  1. Define subagent specifications in `.minicode/agents/*.md` with YAML frontmatter:
     ```yaml
     name: rust-reviewer
     description: Performs strict read-only Rust code review and clippy analysis
     tools: [read_file, grep_search, list_dir, run_cargo_check]
     capabilities: [read_only]
     model: sonnet
     ```
  2. At runtime, when delegating to `rust-reviewer`, `minicode` wraps tool execution in a capability gate. If the model emits a `write_file` tool call, the dispatcher rejects it immediately without execution.
  3. Use `minicode`'s existing `landlock = "0.4"` integration to restrict the process sandbox, ensuring OS-level enforcement against arbitrary filesystem writes outside the repository root.
- **Why It Matters for `minicode`:** Delivers defense-in-depth security, prevents accidental overwrites by specialized agents, and allows safe execution of third-party community subagents.

---

## Architectural Implementation Blueprint for `minicode`

To demonstrate how these 5 ideas integrate into `minicode`'s existing architecture, the following Rust blueprint outlines the core structs and traits:

```rust
// ============================================================================
// minicode Orchestration Architecture: Core Blueprint
// ============================================================================

use std::path::{Path, PathBuf};
use std::sync::Arc;
use async_trait::async_trait;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};

// ----------------------------------------------------------------------------
// 1. Task DAG & State Machine (Ideas 1 & 2)
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Unblocked,
    InProgress,
    CriticReview,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: String,
    pub title: String,
    pub description: String,
    pub assigned_agent: Option<String>,
    pub status: TaskStatus,
    pub complexity_score: u8, // 1 - 10
}

pub struct TaskGraph {
    graph: DiGraph<TaskNode, ()>,
}

impl TaskGraph {
    pub fn new() -> Self {
        Self { graph: DiGraph::new() }
    }

    pub fn get_unblocked_tasks(&self) -> Vec<NodeIndex> {
        self.graph.node_indices().filter(|&idx| {
            let node = &self.graph[idx];
            if node.status != TaskStatus::Pending {
                return false;
            }
            // Unblocked if all incoming dependency nodes are Completed
            self.graph.neighbors_directed(idx, petgraph::Direction::Incoming)
                .all(|dep_idx| self.graph[dep_idx].status == TaskStatus::Completed)
        }).collect()
    }
}

// ----------------------------------------------------------------------------
// 2. Capability Security Model (Idea 5)
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    ReadOnlyFilesystem,
    ReadWriteWorkspace,
    ExecuteShell,
    NetworkAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentSpec {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub capabilities: Vec<Capability>,
    pub model_tier: String,
    pub system_prompt: String,
}

pub struct CapabilityGate;

impl CapabilityGate {
    pub fn verify_access(spec: &SubagentSpec, tool_name: &str, required_cap: Capability) -> Result<(), String> {
        if !spec.allowed_tools.iter().any(|t| t == tool_name) {
            return Err(format!("Agent '{}' is not authorized to use tool '{}'", spec.name, tool_name));
        }
        if !spec.capabilities.contains(&required_cap) {
            return Err(format!("Agent '{}' lacks capability '{:?}' required for tool '{}'", spec.name, required_cap, tool_name));
        }
        Ok(())
    }
}

// ----------------------------------------------------------------------------
// 3. RTK-Style Token Compactor & Output Filter (Idea 3)
// ----------------------------------------------------------------------------

pub trait ToolOutputFilter: Send + Sync {
    fn filter(&self, raw_output: &str) -> String;
}

pub struct CargoCheckFilter;

impl ToolOutputFilter for CargoCheckFilter {
    fn filter(&self, raw: &str) -> String {
        let lines: Vec<&str> = raw.lines().collect();
        let mut filtered = Vec::new();
        let mut capturing_error = false;

        for line in lines {
            if line.starts_with("error[") || line.starts_with("warning[") || line.starts_with("error:") {
                capturing_error = true;
                filtered.push(line);
            } else if capturing_error {
                if line.trim().is_empty() || line.starts_with("Compiling ") || line.starts_with("Checking ") {
                    capturing_error = false;
                } else {
                    filtered.push(line);
                }
            }
        }

        if filtered.is_empty() {
            "Compilation succeeded with zero diagnostics.".to_string()
        } else {
            filtered.join("\n")
        }
    }
}

// ----------------------------------------------------------------------------
// 4. Actor-Critic Verification Loop (Idea 2)
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticVerdict {
    pub approved: bool,
    pub issues_found: Vec<String>,
    pub suggested_remedies: Vec<String>,
}

#[async_trait]
pub trait ActorCriticEngine {
    async fn evaluate_patch(&self, target_file: &Path, diff: &str, test_output: &str) -> CriticVerdict;
}
```

---

## Conclusion & Next Steps

By synthesizing the best architectural paradigms from these 10 industry-leading projects:
- **`minicode` can achieve state-of-the-art multi-agent orchestration** without introducing heavy Python/Node runtimes or sacrificing its single-binary pure-Rust philosophy.
- Implementing the **Top 5 Actionable Ideas** will provide `minicode` with **durable execution**, **resilient multi-agent task planning**, **dramatic 60–90% token reduction**, **background PTY process durability**, and **OS-level Landlock capability sandboxing**.
