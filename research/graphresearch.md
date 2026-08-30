# Deep Technical Research: CodeGraph, Understand-Anything & OKF v0.2 🧠
> **Target:** Architecture, Ideas, Inspirations, and Pure-Rust Integration Strategy for `minicode`  
> **Date:** 2026-08-29  
> **Author:** Antigravity AI Engineering Team  

---

## 1. Executive Summary & Source References

This research synthesizes architectural breakthroughs and semantic knowledge graph designs from three leading open-source projects:

1. **[CodeGraph](https://github.com/colbymchenry/codegraph)** — *The fastest complete code graph & surgical context engine (Rust kernel + SQLite + tree-sitter).*
2. **[Understand-Anything](https://github.com/Egonex-AI/Understand-Anything)** — *Multi-agent codebase and wiki knowledge graph generator with architectural layers, business flows, and guided tours.*
3. **[Open Knowledge Format (OKF v0.2)](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md)** — *Google Cloud's open standard for human- and agent-maintained knowledge bundles with provenance, trust tiers, lifecycle tracking, and progressive disclosure.*

---

## 2. Deep Dive: CodeGraph (Colby McHenry)

### 2.1 Core Problem & Philosophy
AI coding agents traditionally discover codebase structure the slow, expensive way: repeated `grep_search`, `list_dir`, and `read_file` tool calls, manually rebuilding call paths and dependencies over dozens of round trips.
- **The Core Axiom:** *"Surgical context, not file-by-file search."*
- Instead of crawling files, the agent makes **one dense call** to `codegraph_explore` and gets back the exact relevant source code, call paths, dynamic dispatch hops, and change blast radius.

### 2.2 Benchmarks & Measured Wins
Across 7 major repositories (VS Code, Excalidraw, Django, Tokio, OkHttp, Gin, Alamofire):
- **88% fewer tool calls** (e.g. 2 calls vs 28 on VS Code; 2 vs 43 on Excalidraw).
- **53% faster wall-clock time** (2.2× to 3.6× speedup).
- **62% fewer tokens processed** and **44% lower LLM API cost**.
- **0 file reads** needed by the agent on all 7 benchmark codebases.

### 2.3 Rust Kernel Architecture & Scaling
- **Compiled Rust Core:** Tree-sitter parsers for 20 languages (TypeScript, JavaScript, Python, Rust, Go, C/C++, Swift, Java, Kotlin, C#, etc.) with single-boundary crossing per file.
- **Dynamic Resource Sizing:** Adapts worker pools and caches based on actual container/cgroup core counts and real available RAM (handles 70k-file Linux kernel in <12 min on a 2-core VPS).
- **Sub-Second Incremental Watcher:** File changes trigger incremental graph updates in ~300ms using OS notify events without full repo rescans.
- **Dense Tool Pattern (`codegraph_explore`):** Combines search, definition lookup, caller hierarchy, and blast radius into a single consolidated tool payload to eliminate context window "tool description tax".

---

## 3. Deep Dive: Understand-Anything (Egonex AI)

### 3.1 Core Problem & Philosophy
When a developer or AI agent encounters a large unfamiliar codebase (e.g., 200,000 LOC), structural AST graphs alone lack business meaning.
- **The Core Axiom:** *"The goal isn't a graph that wows you with how complex your codebase is — it's a graph that quietly teaches you how every piece fits together."*

### 3.2 Hybrid Multi-Agent Pipeline
1. **Deterministic Parser (Tree-sitter):** Extracts hard static facts (imports, exports, class inheritance, call sites, AST symbols).
2. **Specialized LLM Agents:**
   - `project-scanner`: Discovers languages, frameworks, entrypoints, and package manifests.
   - `file-analyzer`: Extracts functions, classes, dependencies.
   - `architecture-analyzer`: Classifies nodes into architectural layers (**API**, **Service**, **Data**, **UI**, **Utility**).
   - `domain-analyzer`: Maps code entities to real business processes (e.g., *Checkout Flow*, *Authentication Lifecycle*).
   - `tour-builder`: Generates dependency-ordered walkthroughs ("Learn the codebase in the right order").
   - `graph-reviewer`: Validates edge consistency and detects orphan nodes.

### 3.3 Storage & Wiki Support
- **Single File Storage:** Persistent `knowledge-graph.json` in `.ua/` (Git-friendly, diffable).
- **LLM Wiki / Knowledge Base Integration:** Parses Karpathy-pattern markdown wikis with wikilinks (`[[concept]]`), categories, and claim extraction.

---

## 4. Deep Dive: Open Knowledge Format (OKF v0.2 - Google Cloud)

### 4.1 Core Problem & Philosophy
AI agents continuously generate documentation, ADRs, PRDs, and specs, but plain Markdown lacks machine-readable trust, provenance, and lifecycle signals.
- **The Core Axiom:** *"Human-readable, agent-maintainable, diffable in version control, portable without proprietary SDKs."*

### 4.2 Bundle Structure & Reserved Files
```
bundle/
  index.md          # Reserved: Directory listing for progressive disclosure
  log.md            # Reserved: Chronological ledger of all agent/human updates
  <concept>.md      # Self-contained concept document with YAML frontmatter
  subdomain/
    index.md
    <concept>.md
```

### 4.3 Key Metadata Dimensions
1. **Provenance (`sources`):**
   ```yaml
   sources:
     - id: ga4-schema
       resource: https://developers.google.com/analytics/bigquery/export-schema
       title: GA4 BigQuery Export schema
       author: team:ga4-docs
       usage_count: 5000
       last_modified: 2026-05-30T00:00:00Z
   ```
2. **Trust Tiers:** Derived from `verified: { by: "human:aswin", at: "2026-08-29T15:00:00Z" }` (Unverified, Machine-confirmed, Human-reviewed).
3. **Lifecycle:** `generated: { by: "minicode/v0.0.63", at: "..." }`, `status: active|deprecated|superseded`, `superseded_by: ...`.
4. **Attested Computations:** Code definitions that produce verifiable execution receipts.

---

## 5. Architectural Comparison Matrix

| Dimension | CodeGraph (Colby McHenry) | Understand-Anything (Egonex) | OKF v0.2 (Google Cloud) | Proposed `minicode` Implementation |
| :--- | :--- | :--- | :--- | :--- |
| **Primary Domain** | AST Code Graph & Fast Context | Visual Knowledge Graph & Walkthroughs | Knowledge Bundle Standard | High-Performance Agent CLI & TUI |
| **Language/Runtime** | Rust Kernel + Node/Bun CLI | TypeScript / Claude Plugin | Open Markdown + YAML Specification | Pure Rust (Tokio + Petgraph + Tree-sitter) |
| **Storage Engine** | Local SQLite (`.codegraph/`) | Single JSON (`.ua/knowledge-graph.json`) | Plain Markdown files in directory | In-memory Petgraph + `.minicode/graph.json` |
| **Query Strategy** | Dense `codegraph_explore` tool | Interactive Dashboard + Search | Progressive Disclosure (`index.md`) | `code_explore` tool + TUI `/map` modal |
| **Indexing Speed** | Sub-second incremental watch | Multi-agent batch analysis | File-based static authoring | Tree-sitter async worker + notify watcher |
| **Semantic Layers** | Symbol calls & blast radius | API/Service/Data/Domain layers | Concept types, tags, provenance | AstLayers + Symbol Call Paths + Blast Radius |

---

## 6. Actionable Implementation Plan for `minicode` (Pure Rust)

Here is how we can incorporate the best ideas from all three projects into `minicode`:

```
               ┌────────────────────────────────────────────────────────┐
               │                 minicode Codebase                      │
               └──────────────────────────┬─────────────────────────────┘
                                          │
                  ┌───────────────────────┴───────────────────────┐
                  ▼                                               ▼
   ┌───────────────────────────────┐               ┌───────────────────────────────┐
   │ 1. CodeGraph "Dense" Engine   │               │ 2. Understand-Anything Layers │
   │  - tree-sitter AST extraction │               │  - Layer tagging: API/Data/UI │
   │  - Call graph & blast radius  │               │  - Domain process flows       │
   │  - Single `code_explore` tool │               │  - Diff impact analysis       │
   └──────────────┬────────────────┘               └──────────────┬────────────────┘
                  │                                               │
                  └───────────────────────┬───────────────────────┘
                                          ▼
               ┌────────────────────────────────────────────────────────┐
               │ 3. OKF v0.2 Knowledge System (`onpkg_docs/`)           │
               │  - Concept frontmatter (`type`, `sources`, `verified`) │
               │  - Progressive disclosure `index.md` & `log.md`        │
               │  - Autonomous spec-driven verification                 │
               └────────────────────────────────────────────────────────┘
```

---

### Module 1: Dense `code_explore` Tool (`src/tools/explore.rs`, `src/context/graph.rs`)
*Inspired by CodeGraph's `codegraph_explore`*

- **Problem Fixed:** Eliminates 10-30 exploratory `grep_search` and `read_file` calls per task.
- **Tool Signature:**
  ```rust
  pub struct CodeExploreArgs {
      pub query: String,             // e.g. "auth session validation" or "execute_turn"
      pub symbol: Option<String>,    // Optional target function/struct
      pub max_depth: Option<usize>,  // Call graph traversal depth (default: 2)
  }
  ```
- **Return Payload (Single Tool Call):**
  1. **Matched Symbol Definitions:** Complete source code of matched functions/structs with line numbers.
  2. **Call Hierarchy:** Incoming callers (`who calls this?`) and outgoing callees (`what does this call?`).
  3. **Blast Radius Analysis:** List of files, tests, and symbols directly impacted if this symbol is modified.
  4. **Dynamic Dispatch / Trait Implementors:** Resolved trait implementations in Rust / interface implementors in TypeScript.

---

### Module 2: Architectural Layering & Diff Impact (`src/context/repomap.rs`, `src/context/layers.rs`)
*Inspired by Understand-Anything*

- **Architectural Tagging:** Classify every symbol and file into 5 structural tiers:
  - `Layer::Ui` (React components, Ratatui views, templates)
  - `Layer::Api` (HTTP routes, CLI subcommands, RPC handlers)
  - `Layer::Service` (Core business logic, agent loop, scaffolder)
  - `Layer::Data` (Database models, session stores, AST graphs)
  - `Layer::Utility` (Helpers, formatting, error types)
- **Diff Blast Radius (`diff_impact` tool):**
  - Analyzes uncommitted git diffs against the Petgraph code graph.
  - Returns: *"Changing `AgentIntent` in `intent.rs` directly affects `src/main.rs`, `src/app.rs`, and `tests/integration_intent_routing.rs`."*

---

### Module 3: OKF v0.2 Knowledge Bundles in `onpkg_docs/` (`src/context/okf.rs`)
*Inspired by Google Cloud Open Knowledge Format*

- **Standards-Compliant `onpkg_docs/`:**
  - Every file in `onpkg_docs/` (`prd.md`, `design.md`, `implementation.md`, `todo.md`) adopts OKF v0.2 YAML frontmatter.
  - Automatically maintains `onpkg_docs/index.md` (progressive disclosure table of contents) and `onpkg_docs/log.md` (chronological agent change ledger).
- **Provenance & Trust Tracking:**
  - AI-generated documents record `generated: { by: "minicode/v0.0.63", at: "..." }`.
  - When the user reviews or confirms plans, minicode updates frontmatter to `verified: { by: "human:<user>", at: "..." }`.

---

### Module 4: TUI Visual Architecture & Guided Tour Viewer (`/map`, `/explore`)
*Inspired by CodeGraph & Understand-Anything UI*

- **Interactive Ratatui Graph Modal (`ModalState::CodeExplorer`):**
  - **Left Pane:** Architectural layer list (`API`, `Service`, `Data`, `UI`, `Utility`) and detected business flows.
  - **Right Pane:** Call graph tree, symbol definitions with syntax highlighting, and blast radius list.
  - **Keyboard Shortcuts:** `Enter` to jump into source code, `Tab` to toggle callers/callees, `/` to live filter.

---

## 7. Recommended Implementation Sequence

1. **Phase 54 — Dense `code_explore` Engine (P0):**
   - Implement `src/context/explorer.rs` querying `src/context/graph.rs` Petgraph to generate consolidated source + call tree + blast radius payloads.
   - Register `code_explore` in `src/tools/registry/`.
2. **Phase 55 — OKF v0.2 Knowledge System & Auto-Ledgers (P1):**
   - Implement `src/context/okf.rs` for YAML frontmatter parsing, `index.md` progressive disclosure, and `log.md` updates.
3. **Phase 56 — Architectural Layers & Diff Impact Tool (P1):**
   - Add AST classification and `diff_impact` tool to compute blast radius from staged git diffs.
4. **Phase 57 — Interactive TUI Code Explorer Modal (P2):**
   - Build 2-column Aura Ratatui explorer in `src/ui/modal.rs` wired to `/explore` and `Ctrl+E`.
