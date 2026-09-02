# Changelog 📜

All notable changes to **minicode** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.94] — 2026-09-02

### Dynamic Session Memory Checkpointer & Time-Travel Debugger (Phase 74)

#### 💡 Ideas & Inspirations
- **Time-Travel Debugging & State Reversibility**: Long autonomous agent reasoning and editing sequences occasionally make flawed architectural assumptions or go down dead ends. Inspired by Git branch/reflog trees, Redux DevTools Time Travel, and LangGraph checkpointers, `minicode` introduces dynamic immutable state checkpoints and state rewind.
- **Exploration Forking & Branching**: Allows human developers and agents to save intermediate milestones, rewind to earlier conversational turns, or fork parallel exploration hypotheses without losing prior work.

#### 📚 References & Sources
- **LangGraph Checkpoint & State Graph Specification**: State persistence, memory rollback, and thread branching.
- **Redux DevTools Time-Travel Architecture**: Deterministic event-replay and immutable snapshots.
- **Git Tree & Reflog Semantics**: Content-addressed commit DAGs and non-destructive branch resets.

#### 🚀 Features & Changes
- **`SessionCheckpointer` Engine (`src/context/checkpoint.rs`, `src/context/mod.rs`)**:
  - Captures complete immutable state snapshots including conversation event streams and working memory (`TASK_PLAN`, `FINDINGS`, `PROGRESS`).
  - Implements tree-structured checkpoints with `parent_id` tracking, enabling complex branch hierarchies.
  - Provides deterministic state rewind and branch forking operations.
- **`checkpoint_session` & `rewind_session` Agent Tools (`src/tools/registry/context_tools.rs`)**:
  - Registered tools allowing agents to snapshot milestones and list, rewind, or fork checkpoints.
- **Integration Test Suite (`tests/integration_checkpoints.rs`)**:
  - Added 3 integration tests covering snapshot creation, state rewind restoration, and async tool dispatch.

## [0.0.93] — 2026-09-01

### Multi-Workspace Virtual Monorepo Context Orchestrator (Phase 73)

#### 💡 Ideas & Inspirations
- **Holistic Cross-Package Intelligence in Monorepos**: Modern repositories are frequently split into multiple crates or packages (e.g. `crates/core`, `crates/cli`, `packages/web`, `packages/api`). Treating each folder as an isolated silo blinds the agent to cross-package APIs and shared types. Inspired by Turborepo, Nx, and Cargo Workspaces, `minicode` introduces multi-ecosystem virtual monorepo context orchestration.
- **Topological Build Order & Cycle Detection**: Automatically resolves internal package dependency graphs, derives topological compilation order, and flags dangerous circular package dependencies.

#### 📚 References & Sources
- **Turborepo & Nx Monorepo Task Graph Architecture**: Topological DAG execution models.
- **Cargo Workspaces Specification (The Cargo Book)**: Multi-package workspace root and member resolution.
- **npm/pnpm/Yarn Workspaces RFC**: Standardized `package.json` workspace glob definitions.

#### 🚀 Features & Changes
- **`MonorepoOrchestrator` Engine (`src/context/monorepo.rs`, `src/context/mod.rs`)**:
  - Automatically discovers multi-crate Cargo workspaces, npm/pnpm package monorepos, and standalone projects.
  - Resolves internal package-to-package dependency edges and computes topological compilation DAGs.
  - Detects cross-package circular dependencies and generates interactive Mermaid dependency flowcharts.
- **`workspace_monorepo_map` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered tool allowing agents and developers to audit monorepo package boundaries and topological build sequences.
- **Integration Test Suite (`tests/integration_monorepo.rs`)**:
  - Added 3 integration tests covering Cargo workspace member discovery, npm monorepo resolution, and async tool dispatch.

## [0.0.92] — 2026-09-01

### Real-Time Incremental CodeGraph Synchronizer & File Watcher (Phase 72)

#### 💡 Ideas & Inspirations
- **Sub-Millisecond CodeGraph Freshness**: During continuous editing and iterative refactoring loops, rebuilding the entire repository AST graph on every turn introduces latency. Inspired by Salsa incremental computation in `rust-analyzer` and Language Server Protocol (LSP) `textDocument/didChange` syncing, `minicode` introduces sub-millisecond delta AST graph patching.
- **Granular Synchronization Telemetry**: Giving agents and developers visibility into exactly which files changed, which symbol nodes were updated, and the exact sync latency in milliseconds.

#### 📚 References & Sources
- **Incremental Computation & Salsa Framework in rust-analyzer**: Memoization and dirty subtree invalidation architectures.
- **Language Server Protocol (LSP 3.17)**: Incremental document synchronization paradigms.
- **Fast Content Fingerprinting (FNV-1a / xxHash)**: Rapid non-cryptographic cache invalidation.

#### 🚀 Features & Changes
- **`GraphSynchronizer` Engine (`src/context/graph_sync.rs`, `src/context/mod.rs`)**:
  - Leverages cached disk snapshots in `.minicode/graph.json` and selectively re-parses only dirty or newly created files.
  - Automatically prunes deleted file subtrees and associated dependency edges from the graph.
  - Records real-time performance telemetry (`files_scanned`, `files_modified`, `files_added`, `files_deleted`, `sync_latency_ms`).
- **`sync_code_graph` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered tool allowing on-demand incremental updates or forced cold-start rebuilds.
- **Integration Test Suite (`tests/integration_graph_sync.rs`)**:
  - Added 3 integration tests covering full cold rebuilds, incremental delta sync on modified files, and async tool dispatch.

## [0.0.91] — 2026-09-01

### Autonomous Workspace Architecture Documentation Synthesizer (Phase 71)

#### 💡 Ideas & Inspirations
- **Living Architecture Documentation (Docs-as-Code)**: In fast-moving projects and AI-assisted development, static design docs rapidly drift out of sync with actual code implementations. Inspired by Simon Brown's C4 model and dynamic AST extraction, `minicode` introduces zero-maintenance architecture synthesis directly from live AST parse trees and `CodeGraph` dependencies.
- **Visual Mermaid Architecture Graphs**: Generating clean, interactive Mermaid flowcharts of component interactions and Clean Architecture layer boundaries keeps human maintainers and agents aligned without manual diagram drawing.

#### 📚 References & Sources
- **Simon Brown's *C4 Model for Visualizing Software Architecture***: Level 2 & Level 3 component and container diagrams.
- **Mermaid.js Flowchart Specifications**: Declarative graph visualization syntax.
- **Robert C. Martin's *Clean Architecture Layer Classifications***: Layer partitioning matrices.

#### 🚀 Features & Changes
- **`ArchitectureDocSynthesizer` Engine (`src/context/doc_synthesizer.rs`, `src/context/mod.rs`)**:
  - Automatically analyzes workspace source files and classifies them into Clean Architecture tiers (`Presentation`, `Service`, `Domain`, `Data`, `Infrastructure`, `Utility`).
  - Synthesizes Mermaid flowchart diagrams representing data and control flow between architectural subsystems.
  - Catalogs top high-centrality symbols with PageRank rankings to highlight primary system abstractions.
  - Optionally outputs to `ARCHITECTURE.md` directly in the project root.
- **`generate_architecture_docs` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered tool enabling on-demand synthesis and file persistence of living architecture specifications.
- **Integration Test Suite (`tests/integration_doc_synthesizer.rs`)**:
  - Added 3 integration tests covering markdown generation, direct `ARCHITECTURE.md` file writing, and async tool dispatch.

## [0.0.90] — 2026-09-01

### Semantic Code Search & Cross-Encoder Reranker (Phase 70)

#### 💡 Ideas & Inspirations
- **Beyond Keyword Grep and Raw Vector Cosine**: Developers often remember *what* a function does (e.g. *"where is streaming session output flushed?"*) rather than the exact identifier name. Standard vector search frequently returns generic comments or boilerplate. By introducing a two-stage retrieval pipeline (Dense/BM25 candidate retrieval followed by Cross-Encoder AST semantic reranking), `minicode` surfaces the exact implementation logic with high precision.
- **Explainable Match Signals**: Every search result includes explainable relevance factors (e.g. exact phrase match, term coverage %, symbol identifier match, architectural layer alignment, PageRank centrality).

#### 📚 References & Sources
- **Two-Stage RAG & Retrieval Pipelines**: Bi-Encoder candidate retrieval + Cross-Encoder sequence relevance scoring (Cohere Rerank, BGE Reranker architecture).
- **Reciprocal Rank Fusion (RRF)**: Cormack, Clarke, and Buettcher (*Reciprocal Rank Fusion outperforms Condorcet and individual Rank SVM*).
- **Tree-sitter AST & Subword Token Matching**: Structural code decomposition principles.

#### 🚀 Features & Changes
- **`CrossEncoderReranker` Engine (`src/context/reranker.rs`, `src/context/mod.rs`)**:
  - Two-stage code search pipeline integrating `HybridIndex` (BM25 + Semantic Vector embeddings + PageRank) with fine-grained cross-scoring.
  - Computes exact phrase matches, subword token coverage, symbol name alignments, and architectural layer proximity boosts.
  - Formats results into structured `SemanticSearchResult` with confidence scoring and syntax-highlighted snippets.
- **`semantic_code_search` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered tool allowing natural language and technical concept searches with layer filtering.
- **Integration Test Suite (`tests/integration_semantic_search.rs`)**:
  - Added 2 integration tests verifying intent matching over disparate modules and async tool dispatch.

## [0.0.89] — 2026-09-01

### AST-Guided Dead Code & Redundant Symbol Eliminator (Phase 69)

#### 💡 Ideas & Inspirations
- **Zero Technical Debt in Autonomous Development**: As repositories grow and agentic workflows refactor APIs, dead functions and unused structures ("zombie code") accumulate. Inspired by modern bundler tree-shaking (Rollup, Webpack) and compiler mark-and-sweep garbage collection algorithms, `minicode` introduces reachability-based dead code elimination.
- **Dead Island Cluster Isolation**: Unused code often exists in self-referencing clusters (Function A calls Function B, which calls Function A, but neither is ever invoked by the program entrypoints). The reachability engine traces from actual crate roots to isolate entire disconnected subgraphs.

#### 📚 References & Sources
- **Mark-and-Sweep Garbage Collection & Reachability Analysis**: Multi-source breadth-first search traversing downwards from known root roots (`main.rs`, `lib.rs`, `bin/*`, `tests/*`).
- **Tree-Shaking Algorithms (ES Modules & Static AST Analysis)**: Static symbol dependency resolution.
- **Rust `cargo-udeps` & `cargo-deadlinks`**: Principles of dead code and unreferenced item auditing.

#### 🚀 Features & Changes
- **`DeadCodeEliminator` Engine (`src/context/dead_code.rs`, `src/context/mod.rs`)**:
  - Automatically identifies seed entrypoints (`main.rs`, `lib.rs`, exported APIs, test functions).
  - Traverses the entire AST `CodeGraph` using forward reachability BFS.
  - Classifies unreachable items: `DeadFunction`, `DeadStruct`, `DeadEnum`, and `DeadIslandCluster`.
  - Calculates confidence ratings (`HIGH`, `MEDIUM`, `LOW`) and computes potential lines of code (LOC) saved.
- **`dead_code_sweep` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered tool allowing the LLM agent to audit dead symbols and generate cleanup reports.
- **Integration Test Suite (`tests/integration_dead_code.rs`)**:
  - Added 3 integration tests covering unreachable function detection, dead island cluster detection, and tool execution.

## [0.0.88] — 2026-09-01

### Multi-File Symbol Dependency Invariant Checker (Phase 68)

#### 💡 Ideas & Inspirations
- **Preventing Architectural Decay in Agentic Coding**: Autonomous AI coding agents can inadvertently introduce architectural shortcuts (e.g. Domain models invoking UI presentation widgets or Data access layers bypassing domain services). The Invariant Checker provides automated structural guardrails to maintain Clean / Hexagonal Architecture invariants.
- **Fine-Grained Call-Graph Cycles**: File-level import checks miss mutual symbol recursion. Tarjan's Strongly Connected Components algorithm across the symbol graph identifies specific cyclic call paths across files.

#### 📚 References & Sources
- **Robert C. Martin's *Clean Architecture***: Dependency Inversion Principle (DIP) and stable abstraction invariants.
- **ArchUnit Architecture Testing Framework**: Architectural rule declarative validation concepts.
- **Tarjan's SCC (Strongly Connected Components)**: Graph acyclicity verification over `petgraph::algo::tarjan_scc`.

#### 🚀 Features & Changes
- **`InvariantChecker` Engine (`src/context/invariants.rs`, `src/context/mod.rs`)**:
  - `INV-001 (CRITICAL)`: Service $\rightarrow$ UI layer inversion detection (domain calling presentation).
  - `INV-002 (CRITICAL)`: Data $\rightarrow$ UI / API layer inversion detection (persistence calling outward layers).
  - `INV-003 (HIGH)`: Fine-grained multi-file symbol mutual recursion call cycles.
  - `INV-004 (HIGH)`: Utility helpers depending upward on higher architectural tiers.
  - Computes composite Invariant Health Score ($0 \dots 100$) with actionable remediation suggestions.
- **`architecture_invariants` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered tool for the LLM agent and developer to audit repository structural invariants.
- **Integration Test Suite (`tests/integration_invariants.rs`)**:
  - Added 3 integration tests covering layer inversion detection, mutual call cycle detection, and tool dispatch.

## [0.0.87] — 2026-09-01

### Semantic Patch Synthesizer & AST Refactoring Actions (Phase 67)

#### 💡 Ideas & Inspirations
- **Hallucination-Free Code Transformations**: When modifying code, LLMs frequently break indentation, drop comments, or fail to rename all call sites accurately. By equipping `minicode` with deterministic AST refactoring primitives (`extract_function`, `rename_symbol`, `inline_variable`), transformations are syntactically guaranteed to be correct with unified diff generation.
- **Micro-Refactoring Ergonomics**: Inspired by rust-analyzer assists and Martin Fowler's classic refactoring catalog, developers can now safely extract monolithic function blocks or perform cross-file symbol renames in a single automated step.

#### 📚 References & Sources
- **Martin Fowler's *Refactoring: Improving the Design of Existing Code***: Extract Method, Inline Variable, and Rename Field catalogs.
- **rust-analyzer & LSP Code Action Protocols**: Deterministic workspace-wide symbol replacement models.
- **Unified Diff Formats (RFC 6902 / POSIX Diff)**: Standardized atomic diff previews for tool review.

#### 🚀 Features & Changes
- **`AstRefactorer` Engine (`src/context/ast_refactor.rs`, `src/context/mod.rs`)**:
  - `extract_function`: Slices source line ranges, generates new function definitions with explicit parameter and return signatures, and splices the call site with exact indentation preservation.
  - `rename_symbol`: Performs word-boundary AST identifier replacement across scoped files or entire workspace crates.
  - `inline_variable`: Replaces variable usages in function bodies with expression values and safely purges declaration statements.
  - Produces structured `RefactorResult` with unified diff preview.
- **`ast_refactor` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered `ast_refactor` tool for automated code transformation.
- **Integration Test Suite (`tests/integration_ast_refactor.rs`)**:
  - Added 4 integration tests covering function extraction, symbol renaming, variable inlining, and tool dispatch.

## [0.0.86] — 2026-09-01

### Interactive Architectural Heatmap & Call-Graph Visualizer (Phase 66)

#### 💡 Ideas & Inspirations
- **Terminal-Native Visual Architectures**: Developers shouldn't need heavy web browsers or external GUI tools just to understand caller hierarchies, blast radius, and dependencies. Inspired by Unix `tree`, Graphviz DOT representations, and ASCII diagram generators, `minicode` now provides pure-terminal visual graphs.
- **Architectural Heatmap Cards**: Quick-reference box summaries displaying symbol centrality (PageRank), test status, incoming callers, and risk rating in a single glanceable card.

#### 📚 References & Sources
- **Petgraph Directed Graph BFS Traversal**: Upstream & downstream graph exploration using `petgraph::Direction::Incoming` and `petgraph::Direction::Outgoing`.
- **Unicode Box-Drawing Standards**: UTF-8 line drawing (`┌`, `─`, `┐`, `│`, `└`, `┘`, `├──`, `└──`) for seamless TUI rendering.
- **Aider & Tree-Sitter AST Repomap**: Symbolic structural navigation principles.

#### 🚀 Features & Changes
- **`GraphVisualizer` Engine (`src/context/graph_visualizer.rs`, `src/context/mod.rs`)**:
  - Implemented ASCII/Unicode call-tree and architectural box visualizer.
  - Supports multiple visualization modes: `Box` (architectural card), `Upstream` (caller tree), `Downstream` (callee tree), and `Both` (full bidirectional relationship graph).
  - Handles mutual recursive cycle detection and displays `(⟲ mutual cycle)` markers.
- **`graph_visualize` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered `graph_visualize` tool enabling the LLM agent to inspect visual call trees and export architectural cards for any symbol or file.
- **Integration Test Suite (`tests/integration_graph_visualizer.rs`)**:
  - Added 3 integration tests covering box rendering, upstream/downstream tree generation, and async tool dispatch.

## [0.0.85] — 2026-09-01

### AST Code Smells & Architectural Health Linter (Phase 65)

- **`AstSmellDetector` Code Health Engine (`src/context/smell_detector.rs`, `src/context/mod.rs`)**:
  - Implemented multi-dimensional code smell inspection detecting:
    - **God Functions / Monolithic Methods**: Functions exceeding 80 lines (`>80 lines`).
    - **Excessive Parameter Lists**: Functions accepting $\ge 6$ arguments.
    - **Deep Nesting & Complexity**: Indentation depth $\ge 5$ levels (20 spaces or 5 tabs).
    - **Dead Public Exports**: Exported symbols with 0 incoming callers across the `CodeGraph`.
    - **Complex Boolean Logic**: Conditionals with $\ge 4$ logical operators (`&&`, `||`).
  - Computes composite codebase/file **Code Health Score** ($0 \dots 100$) with actionable refactoring remediations.
- **`code_smells` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Exposed tool for the LLM agent to audit architectural quality and request targeted file inspections.
- **Pillar 7 in `/review` Pipeline (`src/git/reviewer.rs`)**:
  - Integrated AST code smell checks into git diff review to proactively flag anti-patterns in newly written code.
- **Integration Test Suite (`tests/integration_code_smells.rs`)**:
  - Added 4 tests verifying god function detection, excessive parameters, deep nesting, and tool dispatch.

## [0.0.84] — 2026-09-01

### Test Gap Detection & Composite Risk Scoring (Phase 64)

- **`TestGapAnalyzer` Call-Graph Reachability Engine (`src/context/test_gap.rs`, `src/context/mod.rs`)**:
  - Implemented multi-source BFS traversing outwards from all discovered test functions and test files across the AST code graph.
  - Accurately classifies every non-test symbol into `DirectlyTested` ($\text{depth} = 1$), `TransitivelyCovered` ($\text{depth} \ge 2$), or `Untested` (zero test reachability).
- **Composite Risk Rating Formula (`src/context/graph.rs`, `src/context/test_gap.rs`)**:
  - Continuous composite risk index: $R = 0.35 \cdot R_{\text{callers}} + 0.25 \cdot R_{\text{pagerank}} + 0.30 \cdot R_{\text{test\_gap}} + 0.10 \cdot R_{\text{pub}}$.
  - Enhanced `BlastRadiusReport` with `composite_risk_score` metric.
- **`test_coverage_gaps` Agent Tool (`src/tools/registry/context_tools.rs`)**:
  - Registered `test_coverage_gaps` tool enabling the LLM agent to inspect missing test coverage, filter by minimum composite risk, and receive suggested integration test suites.
- **Integration Test Suite (`tests/integration_test_gaps.rs`)**:
  - Added integration tests covering test reachability categorization, risk differential, and tool execution.

## [0.0.83] — 2026-09-01

### Progressive Multi-Tier Memory Hierarchy L0 -> L3 (Phase 63)

- **`ProgressiveMemory` 4-Tier Memory Engine (`src/context/progressive_memory.rs`, `src/context/mod.rs`)**:
  - Implemented structured progressive memory hierarchy:
    - **L0 (Working Context)**: Ephemeral in-memory turn scratchpad and active sub-goals.
    - **L1 (Session Anchors)**: Milestone summaries, active goals, and modified file rollups.
    - **L2 (Project Knowledge Base)**: Workspace-persisted architectural facts and conventions (`.minicode/progressive_memory.json`).
    - **L3 (Global Developer Memory)**: Cross-project user preferences and hardware execution constraints (`~/.config/minicode/global_memory.json`).
- **Automatic Rule & Fact Extraction (`src/context/progressive_memory.rs`, `src/agent/loop.rs`)**:
  - Automatically distills guidelines, constraints (e.g. `-j 1` resource limits, commit/push rules), and project facts from turn responses and user prompts.
- **System Prompt Integration (`src/agent/prompt.rs`)**:
  - Injects structured `<progressive_memory>` block into agent system prompt with clean tier separation.
- **Integration Test Suite (`tests/integration_progressive_memory.rs`)**:
  - Added 5 end-to-end tests covering tier separation, local persistence, fact extraction, query retrieval, and prompt block rendering.

## [0.0.82] — 2026-09-01

### Hybrid BM25 + Dense 3-Gram Vector + PageRank RRF Retrieval (Phase 62)

- **`HybridIndex` Multi-Modal Retrieval Engine (`src/context/hybrid.rs`, `src/context/mod.rs`)**:
  - Combined lexical BM25 inverted indexing over identifiers/symbols with dense 128-dimensional 3-gram hashing vector semantic search and `CodeGraph` PageRank graph centrality.
  - Implemented standard **Reciprocal Rank Fusion (RRF)**: $RRF(d) = \sum_{m} \frac{w_m}{k + \text{rank}_m(d)} + w_{\text{pr}} \cdot \text{PageRank}(d)$.
  - Provides multi-source rank tracking and normalized score breakdown for every search hit.
- **RRF Retrieval Constants (`src/constants.rs`)**:
  - Added `RRF_K = 60.0`, `RRF_WEIGHT_LEXICAL = 1.0`, `RRF_WEIGHT_VECTOR = 1.0`, `RRF_WEIGHT_PAGERANK = 0.5`.
- **`hybrid_search` Agent Tool (`src/tools/registry/search_tools.rs`)**:
  - Registered `hybrid_search` schema and dispatcher enabling the agent to execute unified hybrid searches with rich markdown formatting and source attribution.
- **Integration Test Suite (`tests/integration_hybrid_search.rs`)**:
  - Added end-to-end tests covering schema registration, empty workspaces, exact identifier matching, conceptual intent retrieval, and tool dispatch.

## [0.0.81] — 2026-09-01

### Diff-to-Symbol AST Projection for `/review` & `diff_impact` (Phase 61)

- **`DiffProjector` Engine (`src/git/diff_projector.rs`, `src/git/mod.rs`)**:
  - Implemented surgical symbol-level git diff projection mapping raw diff line spans directly to Tree-sitter AST symbol definitions (functions, methods, structs, traits, enums).
  - Categorizes symbol mutations into `Added`, `BodyModified`, `SignatureChanged`, and `Deleted`.
  - Computes symbol-level call-graph blast radius: incoming callers, transitive dependencies, and PageRank architectural scores for every changed symbol.
- **6th Review Pillar in `GitReviewer` (`src/git/reviewer.rs`)**:
  - Added **Structural & Public API Impact Analysis** to the multi-agent code reviewer.
  - Automatically flags breaking public API modifications, deleted symbols with active references, and high-PageRank architectural risk.
- **Enhanced `diff_impact` Tool (`src/tools/registry/explore_tools.rs`)**:
  - Integrated `DiffProjector` into the `diff_impact` tool to provide symbol-level breakdown, caller trees, and downstream file risk alongside file-level analysis.
- **Integration Test Suite (`tests/integration_diff_projector.rs`)**:
  - Added tests covering clean repository projection, added symbols, body vs signature mutations, and the 6th review pillar.

## [0.0.80] — 2026-09-01

### UTF-8 Character Boundary Safety, Subagent Tool Alignment, Confinement & Dependency Pruning

- **Subagent Default Tool Whitelist Alignment (`src/agent/subagent/types.rs`)**:
  - Replaced obsolete legacy tool names (`ripgrep_search` $\rightarrow$ `grep_search`, `symbol_search` $\rightarrow$ `locate_symbol`, `file_structure` $\rightarrow$ `view_outline`) across all subagent role definitions (`Researcher`, `CodeReviewer`, `TestEngineer`, `SecurityAuditor`, `Custom`), fixing tool dispatch failures during subagent swarm delegation.
- **UTF-8 Multi-Byte Character Boundary Panic Prevention (`src/context/recall.rs`, `src/context/semantic.rs`, `src/tools/registry/web_tools.rs`, `src/tools/github/mod.rs`, `src/tools/browser/markdown.rs`)**:
  - Added `content.floor_char_boundary()` to document indexing and chunking in `src/context/recall.rs` to prevent slice panics on multi-byte UTF-8 boundaries.
  - Rewrote 3-gram indexing in `src/context/semantic.rs` using safe character window iteration.
  - Applied `floor_char_boundary` to preview truncation in `src/tools/registry/web_tools.rs` and log tail slicing in `src/tools/github/mod.rs`.
  - Replaced case-conversion slicing in `src/tools/browser/markdown.rs` with `find_ascii_ci` helper using `.eq_ignore_ascii_case()` to prevent panics and character corruption.
- **Suffix Truncation Logic Fixes (`src/context/okf.rs`, `src/tools/github/client.rs`)**:
  - Replaced `trim_end_matches(".md")` with `.strip_suffix(".md")` in `src/context/okf.rs` to prevent corrupting filenames ending with letters in `m`, `d`, or `.`.
  - Replaced `trim_end_matches(".git")` with `.strip_suffix(".git")` in `src/tools/github/client.rs`.
- **CDP Driver Channel & Subagent Timeout Fixes (`src/tools/browser/driver.rs`, `src/agent/subagent/mod.rs`)**:
  - Removed duplicate disconnected `console_log` allocation in `CdpClient::connect` so console events flow to the active reader.
  - Added explicit `success = false` on subagent timeout expiry in `SubAgent::run_task`.
- **Workspace Sandbox Confinement & SSRF Hardening (`src/tools/browser/mod.rs`, `src/tools/onpkg/scaffolder.rs`, `src/tools/onpkg/skills.rs`, `src/context/ast_transform.rs`, `src/tools/crawler/engine.rs`)**:
  - Enforced `validate_path_in_workspace` on `file://` URLs in browser tools, stack scaffolder target paths, and AST transform node queries.
  - Validated skill package names against path separators and traversal in `onpkg_skill_install`.
  - Enforced `validate_ssrf` on crawl root and discovered links in `crawler/engine.rs`.
- **Subagent Configuration Forwarding & Auto-Compaction Pruning (`src/agent/subagent/mod.rs`, `src/agent/orchestrator.rs`, `src/tools/registry/agent_tools.rs`, `src/context/auto_compact.rs`)**:
  - Added `with_config` and `delegate_with_config` to forward subagent models and budgets to worker processes.
  - Hardened Tier 3 compaction in `auto_compact.rs` to prune orphaned `Role::Tool` messages at both index 0 and index 1 after system summaries.
- **UI & Palette Command Alignment & Scroll Deduplication (`src/ui/input.rs`, `src/ui/modal.rs`, `src/ui/view.rs`)**:
  - Synchronized `/commands`, `/map`, `/save`, `/load`, `/provider` between palette and catalog.
  - Extracted shared `compute_scroll_offset` helper in `src/ui/modal.rs`, deduplicating viewport scrolling calculations across 9 modal types.
  - Fixed mouse hit testing using `area.right()` / `area.bottom()` and saturating line folding math in `src/ui/view.rs`.
  - Added underflow-safe category cycling in `src/ui/input.rs`.
- **Dependency Pruning & Test Suite Hardening (`Cargo.toml`, `tests/`)**:
  - Pruned 9 unused dependencies (`rmcp`, `lsp-types`, `grep-regex`, `grep-searcher`, `glob`, `colored`, `indicatif`, `terminal-colorsaurus`, `anstyle-query`).
  - Added explicit panic branches to `if let` blocks in integration tests to prevent silent test passes.

## [0.0.79] — 2026-09-01

### Crash-Risk Overflow Fixes, Viewport Scroll Standardization, SSRF Hardening & Code Health

- **Crash-Risk Overflow Prevention (`src/ui/view.rs`, `src/ui/input.rs`)**:
  - Fixed `visual_row_count` integer overflow by accumulating into `usize` and clamping to `u16::MAX`, preventing panics during long sessions or large diff outputs.
  - Cast `area.width` to `u32` during command palette width calculation to prevent `u16` multiplication overflow on wide terminals.
- **Viewport Scroll Standardization & Bug Fixes (`src/ui/modal.rs`)**:
  - Standardized all modals (`UndoCheckpoint`, `ThemeSelect`, `SessionBrowser`, `CommandCatalog`, `ProviderSelect`) to use the safe `saturating_sub` scroll pattern with `.max(1)` bounds.
  - Added viewport windowing to `ProviderSelect` modal.
  - Switched `CommandCatalog` search counter padding to `UnicodeWidthStr::width()`.
  - Added line number gutter formatting to `CodeExplorer` source code inspection tab.
- **Palette & Catalog Command Alignment (`src/ui/input.rs`, `src/ui/modal.rs`)**:
  - Aligned all slash command shortcuts between `PALETTE_COMMANDS` and `COMMAND_CATALOG_ITEMS` (added `Ctrl+U` for `/undo`, aligned `F2`, `F5`, `Ctrl+H`, `Ctrl+R`, `Ctrl+L`, `Ctrl+N`, `Ctrl+D`, `Ctrl+T`, `F1`, `Ctrl+C`).
- **Data Durability & Security Hardening (`src/session/store.rs`, `src/constants.rs`, `src/tools/github/client.rs`)**:
  - Switched `create_session` from `.create(true).truncate(true)` to `.create_new(true)` to prevent accidental overwrite on collision.
  - Added `sync_all()` on markdown export to ensure disk durability.
  - Added `"metadata.google.internal"` and `"instance-data"` to `SSRF_BLOCKED_HOSTS`.
  - Replaced hardcoded GitHub client user-agent version with dynamic `CARGO_PKG_VERSION`.
- **DRY & Code Health Polish (`src/constants.rs`, `src/ui/input.rs`, `src/ui/view.rs`)**:
  - Extracted 9 modal percentage dimension pairs into `src/constants.rs`.
  - Extracted shared `SPINNER_FRAMES` constant and unified across all 3 view branches.
  - Extracted `InputDock::reset()` helper method for clean prompt resets.
  - Derived `CHECKPOINT_PROMPT_PREVIEW_CHARS = CHECKPOINT_PROMPT_MAX_CHARS - 3`.
  - Pruned unused constants and imports.

## [0.0.78] — 2026-08-31

### Critical Stability Hardening, Unicode Safety & Comprehensive Code Health

- **Underflow Panic Fix (`src/ui/input.rs`)**:
  - Fixed an arithmetic underflow panic when computing command palette scroll offset (`list_height - 1`) on small or compressed terminal heights.
- **Display-Width Safe Alignment (`src/ui/input.rs`)**:
  - Switched shortcut badge spacing calculation from raw byte length (`len()`) to `UnicodeWidthStr::width()` to avoid misalignment caused by multi-byte symbols like `❯`.
- **Session Store Hardening (`src/session/store.rs`)**:
  - Fixed preview overwrite bug where `turn_start` or `stream_delta` blocked user prompts from appearing in session history.
  - Added `file.sync_data()` after each JSONL event write to ensure zero data loss on abrupt termination.
  - Replaced session ID blacklist validation with strict character allowlist (`is_ascii_alphanumeric() || '-' || '_'`).
  - Eliminated TOCTOU check-then-act races across file loaders and directory readers.
  - Added warning logging for corrupted `session_meta` lines and file open failures instead of silent swallowing.
  - Removed redundant secondary sorting in `get_last_session_id`.
  - Added buffer flush before `FileModified` and `GitCommit` in `export_markdown`.
- **Viewport Scrolling across all Modals (`src/ui/modal.rs`)**:
  - Added smooth viewport windowing to `ModelSelect`, `StackSelect`, `CodeExplorer`, and `GitDiff`.
  - Normalized search filter strings with `.trim()` across all interactive filterable modals.
  - Replaced hardcoded dimensions and magic numbers with centralized constants in `src/constants.rs`.
- **UI Dynamic Layouts (`src/ui/view.rs`)**:
  - Replaced static 64-character border with dynamically sized area-aware bottom card borders.
- **Dead Code Cleanup (`src/ui/input.rs`)**:
  - Removed obsolete `autocomplete_slash` function and unified placeholder text with `DEFAULT_INPUT_PLACEHOLDER`.

## [0.0.77] — 2026-08-31

### Universal Menu Decluttering & Minimalist UI Polish

- **Clean Minimalist Modals & Menus (`src/ui/modal.rs`, `src/ui/input.rs`)**:
  - Removed cluttered and redundant bottom footer keycap shortcut hint bars across all interactive TUI menus and modals:
    - `SessionBrowser` (`/sessions`, `/history`, `Ctrl+H`): Removed `[Enter] Resume • [f] Fork • [e] Export MD • [d] Delete • [Esc] Close`. Full card inner area is now dedicated to the session timeline.
    - `ExitConfirm` (`/exit`, `/quit`, `Ctrl+C`): Removed bottom footer line `Press [Y] to quit • [N/Esc] to stay • [Ctrl+C ×2] force`. Adjusted card height to 8 lines for a compact, ultra-clean popup.
    - `CommandPalette` (`/` popup in input dock): Removed bottom divider and footer hint `↑/↓ browse • Enter select • Tab switch category • Esc dismiss`. Adjusted palette height to 10 lines.
    - `ModelSelect` (`/models`, `/model`): Removed footer keybind hints row.
    - `UndoCheckpoint` (`/undo`): Removed footer action bar.
    - `ThemeSelect` (`/theme`): Removed footer action bar.
    - `StackBrowser` (`/stacks`, `/scaffold`): Removed footer action bar.
    - `CommandCatalog` (`/commands`, `/help`): Removed footer action bar.
    - `GitDiff` (`/diff`): Removed footer hotkeys bar and directly rendered 2-column files & diff viewer.
    - `CodeExplorer` (`/explore`, `Ctrl+E`): Removed footer action bar.

## [0.0.76] — 2026-08-31

### Minimal Floating Spotlight Session History (Option 1)

- **Clean Floating Spotlight Session History (`src/ui/modal.rs`)**:
  - Redesigned the Session Browser modal (`/sessions`, `/history`, `Ctrl+H`) from a rigid full-screen 2-column pane into a centered, rounded floating spotlight card (`BorderType::Rounded`).
  - Integrated header with session count indicator: `╭─ 📜 Session History ───────────── [1/14] ─╮`.
  - Replaced raw, noisy timestamp UUIDs (`20260831T142903Z-...`) with human-readable task/prompt titles (e.g. `› 13s ago   Fix permission asking modal responsive layout`).
  - Two-line structured session cards with event statistics, model information, and subtle relative time ago badges.
  - Smooth viewport windowing so large session archives scroll seamlessly.
  - Modern bottom keycap shortcut action guide: `[Enter] Resume  •  [f] Fork  •  [e] Export MD  •  [d] Delete  •  [Esc] Close`.

## [0.0.75] — 2026-08-31

### Informative Developer Exit Card (Variant 3) & Theme Token Binding

- **Informative Developer Session Exit Card (`src/ui/modal.rs`)**:
  - Implemented **Variant 3: Informative Session Card** matching minicode's signature developer aesthetic.
  - Displays live workspace information and version badge: `● Workspace: <workspace_name> (v0.0.75)`.
  - Reassures developer of session persistence: `● Session: auto-saved to .minicode/history`.
  - Clear, distinct action pills: `[ ✖ Yep, Quit (Y) ]` and `[ ✔ Stay in Session (N) ]`.
  - Fully binds to active Theme tokens (`theme.destructive` for Quit, `theme.brand_accent` for Stay, `theme.bg_elevated` for modal surface).
  - Modern footer keycap bar: `Press [Y] to quit • [N/Esc] to stay • [Ctrl+C ×2] force`.

## [0.0.74] — 2026-08-31

### Interactive Exit Confirmation Menu (`src/ui/modal.rs`, `src/app.rs`)

- **Interactive Exit Confirmation Dialog (`src/ui/modal.rs`)**:
  - Implemented sleek floating exit confirmation card matching modern TUI design.
  - Question: `"Are you sure you want to quit?"`
  - Interactive button row: `[ Yep! ]` and `[ Nope ]` with safe default focus on `Nope`.
  - Vibrant magenta/pink pill highlight (`#ff5ad2`) on focused button and hotkey underlining (`Y` in `Yep!`, `N` in `Nope`).
  - Subtext hint: `"To quit without confirmation press ctrl+c twice."`
- **Smart Exit Navigation & Keybindings (`src/app.rs`)**:
  - Arrow keys (`Left`/`Right`) or `Tab`/`Shift+Tab` smoothly toggle selection between `Yep!` and `Nope`.
  - Direct single-key hotkeys: press `y` to confirm quit, `n` or `Esc` to cancel.
  - Rapid double `Ctrl+C` (< 1.5s) immediately exits without confirmation.
  - Triggered automatically via `Ctrl+C`, `/exit`, `/quit`, or selecting "Exit minicode" from the Command Palette.

## [0.0.73] — 2026-08-31

### Floating Spotlight Command Palette (Option 1) & Clean Human-Readable Registry

- **Clean Floating Spotlight Command Palette (`src/ui/input.rs`, `src/app.rs`)**:
  - Automatically pops up the modern floating spotlight Command Palette when typing `/` in the input prompt.
  - Implemented **Option 1: Clean Floating Spotlight** with rounded corners (`BorderType::Rounded`), brand accent border, and live search input row (`› /search█`).
  - Top category radio pills (`[Tab]` switches between `◉ All`, `○ System`, `○ Intelligence`, and `○ Tools`).
  - Real-time search query filtering against command titles, descriptions, slash names, and shortcuts.
  - Full-line vibrant highlight on active item (`theme.brand_accent` background, bold foreground).
  - Right-aligned shortcut keycap badges (`ctrl+n`, `ctrl+l`, `ctrl+r`, `ctrl+e`, `ctrl+t`, `F5`, `F1`, `ctrl+c`).
  - Viewport scrolling with windowing support so lists of any length navigate smoothly.
- **Dynamic Non-Hardcoded Command Registry (`src/ui/input.rs`)**:
  - Replaced ambiguous text items with clean, human-readable command titles (`New Session`, `Switch Model`, `Repository Index`, `Code Review`, `Configure Providers`, `Clear Timeline`, etc.) mapped directly to real minicode features.
- **Global Shortcut Keybindings (`src/app.rs`)**:
  - Added global `Ctrl+N` for instant new session reset.
  - Added global `Ctrl+L` for model & provider switching.
  - Added global `Ctrl+R` for git review.
  - Added global `F1` for help shortcuts cheatsheet and `F5` for workspace analysis.

## [0.0.72] — 2026-08-31

### Ultra-Compact Permission Dialog (Variant 2A) & Argument Extraction Fix

- **Ultra-Compact Permission Dialog (`src/ui/approval.rs`)**:
  - Implemented **Variant 2A: Modern Rounded & Pill-Highlighted Single-Block Strip** (reduced height from 30+ empty lines down to a sleek 8–9 lines).
  - Added rounded border styling with brand accent styling (`BorderType::Rounded`).
  - Added full-line vibrant pill highlight on active selection (`theme.brand_accent` background + bold text).
  - Auto-responsive width (`(width - 4).clamp(48, 62)`) and dynamic height fitting the content cleanly over the timeline.
- **Command & Tool Argument Resolution Fix (`src/ui/approval.rs`)**:
  - Fixed argument extraction for `exec_cmd` to check `"command"` and `"cmd"`, displaying exact highlighted shell commands (`▶ Run: $ cargo test -j 1`).
  - Fixed tool-aware badges for `patch_file` (`📝 Patch: src/main.rs (+5/-2 lines)`) and `write_file` (`📄 Write: path (KB)`).
  - Gracefully truncates long commands and paths with ellipsis (`…`) so the modal borders never break.
- **Instant Hotkey Response (`src/ui/approval.rs`, `src/app.rs`)**:
  - Added instant key triggers: press `y` (Approve), `n` / `Esc` (Reject), `a` (Always allow for session), or `f` (Type custom feedback).

## [0.0.71] — 2026-08-31

### Global Non-Destructive Configuration, Multi-Provider Support & Graceful Startup

- **Global Configuration Non-Destructive Persistence (`src/config.rs`, `src/ui/configure.rs`)**:
  - Added `[provider.api_keys]` and `[provider.custom_endpoints]` persistent tables in `~/.config/minicode/config.toml`.
  - Added global environment key synchronization into `~/.config/minicode/.env`, automatically loaded on every minicode launch regardless of the current working directory.
  - Enhanced `localupdate.sh` with automatic safety backup of existing `.env` files in `$CONFIG_DIR/backups` with 10-backup rotation retention.
- **Graceful Startup Handling (`src/main.rs`)**:
  - Eliminated missing API key startup crashes in interactive mode; now welcomes the user and opens the interactive setup wizard immediately.
  - If setup is aborted, exits cleanly with zero error code and helpful reminder (`minicode configure`).
  - Added `minicode config` and `minicode setup` as CLI aliases for `minicode configure`.
- **Expanded Provider Ecosystem (`src/agent/provider.rs`, `src/agent/models.rs`, `src/constants.rs`, `src/ui/modal.rs`)**:
  - Added first-class support for **MiniMax** (`https://api.minimaxi.chat/v1`, `MiniMax-Text-01`), **Z.ai / Zhipu GLM** (`https://open.bigmodel.cn/api/paas/v4`, `glm-4-flash`), and **Mistral AI** (`https://api.mistral.ai/v1`, `codestral-latest`).
  - Added live model fetching, context window discovery, and free-tier badges (`[FREE]`) across OpenRouter, Gemini, OpenAI, DeepSeek, Groq, MiniMax, Zhipu, Together, Mistral, Ollama, and custom endpoints.
- **Interactive Configuration Submenus (`src/ui/configure.rs`)**:
  - Added option `[2] 🔑 Manage / Reconfigure API Keys` to view and update any provider key without switching the active provider.
  - Added `[0] / b` back options at every menu level and `del` command to clear stored keys.
  - Added custom OpenAI-compatible endpoint registration (vLLM, LM Studio, Ollama, LocalAI, Azure).

## [0.0.70] — 2026-08-31

### Interactive Workspace Analysis Onboarding Menu & Cache-Aware Indexing

- **Workspace Analysis Onboarding Modal (`src/ui/modal.rs`)**:
  - Implemented `ModalState::WorkspaceAnalysis` with Design 1 minimal rounded card layout.
  - Automatically activates on initial startup in unindexed repositories to give users explicit control over indexing operations.
  - In unindexed mode, presents options: `[1] ⚡ Quick Index` (AST symbols + PageRank graph), `[2] 🧠 Deep Scan` (Symbol graph + Architecture scan), and `[3] ⏩ Skip` (Instant lightweight chat).
  - In indexed mode, presents options: `[1] ⚡ Incremental Sync` (~5ms modified file scan), `[2] 🔄 Full Rebuild` (Cold re-index), `[3] 🗺️ View Repo Map` (PageRank centrality hierarchy), and `[4] ⏩ Close`.
- **Keyboard Shortcuts & Commands (`src/app.rs`)**:
  - Bound `F2` key to open the interactive Workspace Analysis modal anytime.
  - Added slash commands `/init`, `/index`, and `/analyze` to open the modal or run cache-aware analysis.
  - Intercepts natural language keywords (`"analyze the project"`, `"index codebase"`, `"generate code graph"`, `"reindex"`) to trigger cache-aware indexing.
- **Resumed Session Safety (`src/app.rs`, `src/main.rs`)**:
  - Session hydration skips the onboarding modal so resumed past conversations open straight into active chat.

## [0.0.69] — 2026-08-31

### Code Graph Disk Persistence & Incremental Snapshot Caching

- **Graph Storage Engine (`src/context/graph_store.rs`)**:
  - Introduced `GraphStore` and `GraphSnapshot` serialization format with versioned schema (`SCHEMA_VERSION = 1`).
  - Implemented atomic disk serialization via UUID-tagged temporary files and atomic rename to `.minicode/graph.json`.
  - Added safety guardrails with `MAX_GRAPH_FILE_SIZE_BYTES` (10 MB) threshold to prevent unbounded growth.
- **Warm-Start Graph Caching & Lifecycle Integration (`src/context/graph.rs`)**:
  - `CodeGraph::build_graph()` now leverages `load_cached()` on startup for instantaneous warm starts, followed by fast `incremental_update()`.
  - Automatically serializes the updated graph back to disk via `save_to_disk()`.
- **Incremental Update Engine (`src/context/graph.rs`)**:
  - Added `incremental_update()` calculating `IncrementalStats` (`files_scanned`, `files_reparsed`, `files_removed`, `nodes_added`, `nodes_removed`, `edges_rebuilt`).
  - Incrementally purges removed and stale symbols while re-wiring callers and file dependencies in milliseconds.

## [0.0.68] — 2026-08-31

### Symbol-Level Code Graph & Incremental Change Tracking

- **Symbol-Level Petgraph Architecture (`src/context/graph.rs`)**:
  - Replaced file-level `DiGraph<PathBuf, ()>` with fine-grained symbol-level `DiGraph<SymbolNode, EdgeKind>`.
  - Added `SymbolKind` enumeration covering 14 symbol categories (`Function`, `Method`, `Struct`, `Class`, `Trait`, `Interface`, `Enum`, `TypeAlias`, `Impl`, `Import`, `Module`, `Variable`, `File`, `Other`).
  - Added `SymbolNode` containing symbol identifier, qualified path (`path::symbol`), kind, line spans, signature, and doc comments.
  - Added `EdgeKind` multi-graph relationship taxonomy (`Calls`, `Imports`, `Implements`, `Contains`, `References`, `DependsOn`).
- **Incremental File Hash Tracking (`src/context/graph.rs`)**:
  - Introduced `FileHashTracker` computing pure Rust FNV-1a 64-bit content hashes alongside filesystem modification timestamps.
  - Enables incremental re-indexing where only modified files are reparsed, dramatically accelerating graph updates.
- **Symbol-Precision PageRank & Blast Radius (`src/context/graph.rs`)**:
  - Added `compute_symbol_pagerank()` ranking individual functions and types by centrality, with backward-compatible file-level aggregation (`compute_pagerank()`).
  - Enhanced `get_blast_radius()` to return exact caller symbol names (`direct_caller_symbols`, `transitive_caller_symbols`) in addition to dependent file paths.
- **Constants & Code Health (`src/constants.rs`)**:
  - Added `SYMBOL_GRAPH_MAX_NODES` (50,000), `SYMBOL_GRAPH_MAX_EDGES` (200,000), and `SYMBOL_REFERENCE_MIN_LEN` (3).
  - Maintained zero clippy warnings with `-D warnings` and 100% test pass rate.

## [0.0.67] — 2026-08-30

### Unicode Safety, Timeout Standardization & Hardening Polish

- **Full-Spectrum Unicode Safety (`src/tools/onpkg/doctor.rs`, `src/ui/modal.rs`)**:
  - Replaced byte-slice truncation `&first_line[..40]` in `doctor.rs` with safe char-iterator collection `first_line.chars().take(40).collect()`, preventing panics on multibyte UTF-8 characters from external CLI tools.
  - Replaced byte-slice truncation `&cp.prompt[..52]` in `src/ui/modal.rs` with safe `cp.prompt.chars().take(52).collect::<String>()`, ensuring undo timeline never panics when previewing non-ASCII prompt text or emojis.
- **HTTP Timeout Standardization (`src/tools/web_search.rs`, `src/tools/crawler/engine.rs`, `src/tools/registry/web_tools.rs`)**:
  - Replaced hardcoded `10s` and `12s` timeouts and user agent literals with centralized `crate::constants::WEB_TIMEOUT_SECS` (15s) and `crate::constants::WEB_USER_AGENT`.
- **Zero Duration Polish (`src/context/decay.rs`, `src/context/wiki.rs`)**:
  - Replaced `Duration::from_secs(0)` fallbacks with idiomatic `Duration::ZERO`.

## [0.0.66] — 2026-08-30

### Code Health, Panic Hardening, Deterministic Eviction & Performance Optimizations

- **Zero-Panic Guarantee & Rule Compliance (`src/agent/loop.rs`, `src/tools/web_search.rs`, `src/agent/task_dag.rs`, `src/tools/rtk_filter.rs`)**:
  - Replaced forbidden `.expect("fallback compactor")` in `AgentLoop::new()` with infallible `AutoCompactor::default_safe()`.
  - Replaced 4× `.unwrap()` in `src/tools/web_search.rs` with safe `?` error propagation and `ToolError::CommandExec` mapping.
  - Replaced `.unwrap()` on `child_ids.last()` in `src/agent/task_dag.rs` with safe `Option` matching.
  - Replaced static `.expect()` regex in `src/tools/rtk_filter.rs` with infallible `OnceLock<Option<Regex>>`.
  - Audited and verified **zero `.unwrap()` and zero `.expect()` in non-test library code**.
- **Deterministic MemoryAnchor Eviction (`src/context/auto_compact.rs`, `indexmap`)**:
  - Switched `MemoryAnchor.file_state` from `std::collections::HashMap` to `indexmap::IndexMap`.
  - Uses `shift_remove_index(0)` for deterministic FIFO eviction of oldest modified files when exceeding `COMPACT_MAX_FILES_IN_ANCHOR`.
- **Cancellation-Aware Rate Limit & Retry Sleeps (`src/agent/loop.rs`)**:
  - Wrapped retry backoff delays in `tokio::select!` with `cancel_token.cancelled()` to immediately interrupt on `Esc` / `Ctrl+C` without blocking.
- **UTF-8 Safe String Slicing (`src/app.rs`)**:
  - Fixed byte-index string slicing in clipboard copy preview to use `.chars().take(25).collect::<String>()`, eliminating potential panics on multi-byte characters.
- **Resource Protection & Bounded Reads (`src/agent/loop.rs`, `src/constants.rs`)**:
  - Added `MAX_DIFF_SNAPSHOT_BYTES` (512 KB) file size check before reading files for inline diff snapshots, preventing memory spikes on large or binary files.
- **Zero-Allocation Modal Filtering (`src/ui/modal.rs`)**:
  - Replaced per-keystroke `.to_lowercase()` string allocations across models, stacks, command catalog, and code explorer with zero-allocation ASCII case-insensitive windowing (`contains_ci`).
- **PageRank Allocation Reuse (`src/context/graph.rs`)**:
  - Pre-allocated `next_scores` HashMap and swapped buffers across PageRank iterations, eliminating memory churn on large dependency graphs.
- **Constants Extraction (`src/constants.rs`)**:
  - Extracted 15+ hardcoded heuristics to named constants: `COMPACT_MAX_DECISIONS_PER_TURN`, `COMPACT_MAX_ERRORS_PER_TURN`, `COMPACT_MAX_DECISION_CHARS`, `COMPACT_MAX_ERROR_CHARS`, `MESSAGE_FRAMING_TOKEN_OVERHEAD`, `MIN_COMPACTABLE_MESSAGES`, `MIN_LINES_FOR_DEDUPLICATION`, `FNV_OFFSET_BASIS`, `FNV_PRIME`, `MAX_MATCHED_ENTRIES`, `MAX_SOURCE_LINES`, `MAX_CALLERS`, `MAX_CALLEES`, `TOTAL_TOOL_COUNT`, `SPINNER_FRAME_MS`, `SCROLL_LINES_NORMAL`.
- **Robust Model Pricing & Flag Matching (`src/agent/models.rs`, `src/app.rs`)**:
  - Replaced fragile string comparison `== Some("0")` with numeric float parsing `Ok(0.0)` / `Some(0.0)`.
  - Replaced substring `.contains("-s")` with word-boundary matching `split_whitespace().any(...)`.

## [0.0.65] — 2026-08-30

### Smart Context Window Auto-Compaction Engine & Memory Anchors

- **4-Tier Progressive Auto-Compaction (`src/context/auto_compact.rs`, `AutoCompactor`)**:
  - Replaces naive message dropping with structured progressive compression:
    - **Tier 0 (Always)**: Preserves most recent 3 turns (6 messages) untouched at 100% full fidelity.
    - **Tier 1 (60% Context Limit)**: Masks older verbose tool observations and compiler outputs to head+tail lines.
    - **Tier 2 (80% Context Limit)**: Algorithmically extracts structured `TurnSummary` blocks (files read, files modified, key decisions, resolved errors, tools used) and collapses older turns into compact markdown blocks without extra LLM API calls.
    - **Tier 3 (95% Context Limit)**: Synthesizes a persistent `MemoryAnchor` and prunes older messages down to budget while preserving all critical architectural decisions and file states.
- **Adaptive Model-Aware Thresholds (`src/agent/models.rs`, `src/constants.rs`)**:
  - Dynamically calculates absolute token thresholds based on model context limits (Liquid/North 65K, GPT-4o 128K, Claude 200K, Gemini 1M, Gemma 8K).
  - Keeps smaller and free-tier models well within context boundaries without context window overflow errors.
- **Persistent Memory Anchor (`MemoryAnchor`, `src/agent/prompt.rs`)**:
  - Rolling session memory block injected into system prompts capturing active goal, key decisions, tracked file states, and unresolved issues across compaction boundaries.
- **Context Compaction Visibility & Telemetry (`AgentEvent::ContextCompacted`, `src/agent/types.rs`, `src/ui/view.rs`)**:
  - Emits structured compaction telemetry over MPSC and NDJSON streams.
  - Renders visual timeline badge in TUI: `🗜️ Context Auto-Compacted [Tier 2]: 4 turns summarized, 52.0k → 18.5k tokens (64% saved)`.
- **Enhanced `/compact` Slash Command (`src/app.rs`)**:
  - Displays interactive context window health status with token limits and progressive tier thresholds.
- **Integration Test Suite (`tests/integration_auto_compact.rs`)**:
  - 7 comprehensive tests covering all 4 compaction tiers, model-aware threshold scaling, memory anchor prompt injection, and event serialization.

## [0.0.64] — 2026-08-30

### CodeGraph Surgical Context, Architectural Layers & OKF v0.2 Knowledge System

- **CodeGraph Surgical Context Explorer (`src/context/explorer.rs`, `code_explore` tool)**:
  - Dense 1-call context retrieval combining verbatim symbol definitions, signatures, doc comments, incoming callers (`← who calls this`), outgoing callees (`→ what does this call`), and blast radius.
  - Eliminates iterative multi-turn search/read cycles, saving 60-80% of agent context tokens during code exploration.
- **Architectural Layer Classification (`src/context/layers.rs`, `ArchitecturalLayer`)**:
  - Automatically classifies files and AST symbols into 5 architectural tiers: `🎨 UI`, `🌐 API`, `⚙️ Service`, `💾 Data`, and `🔧 Utility`.
  - Enriches repomap representations and code exploration with visual layer badges.
- **Uncommitted Diff Blast Radius & Impact Analysis (`diff_impact` tool)**:
  - Analyzes active git modifications or specified file paths against the AST dependency graph to compute blast radius, downstream dependents, affected architectural tiers, and required regression test coverage.
- **Open Knowledge Format (OKF v0.2) Documentation System (`src/context/okf.rs`)**:
  - Full YAML frontmatter parser and validator according to Google Cloud OKF v0.2 specifications.
  - `OkfManager` auto-generates progressive disclosure table-of-contents (`onpkg_docs/index.md`) and chronological audit ledgers (`onpkg_docs/log.md`).
  - Integrated into `OnpkgSyncEngine` to keep project documentation continuously synchronized.
- **Interactive TUI Code Explorer Modal (`ModalState::CodeExplorer`, `/explore`, `Ctrl+E`)**:
  - 2-column interactive TUI browser with live search/filter by symbol name, signature, file, and layer.
  - 4 inspection tabs: `[1. Source Definition]`, `[2. Callers]`, `[3. Callees]`, and `[4. Blast Radius]`.
  - Registered `/explore` in `SLASH_COMMANDS`, `COMMAND_CATALOG_ITEMS`, and autonomous intent router (`AgentIntent::CodeExplore`).
- **Integration Test Suite (`tests/integration_code_explore.rs`)**:
  - 5 comprehensive tests covering layer classification, surgical symbol exploration with caller detection, diff impact blast radius, OKF v0.2 frontmatter parsing and ledger generation, and intent routing.

## [0.0.63] — 2026-08-29

### Autonomous Intent Routing & Interactive Command Catalog

- **Autonomous Intent Routing Engine (`src/agent/intent.rs`, `src/agent/mod.rs`)**:
  - Implemented `AgentIntent` classification system across 10 core domains (`StackScaffold`, `MilestonePlan`, `AutonomousGoal`, `CodeReview`, `GitDiff`, `SessionHistory`, `UndoRollback`, `RepoMap`, `ContextCompact`, `CommandCatalog`).
  - Zero-latency heuristic matcher with automatic query extraction and confidence scoring.
  - Live timeline intent feedback badges in interactive TUI (e.g. `📋 Autonomous Intent: Milestone Planning`).
- **Interactive Command Catalog Modal (`ModalState::CommandCatalog`, `/commands`, `src/ui/modal.rs`)**:
  - 2-column searchable command browser with live filter bar, category tags, shortcuts (`Ctrl+D`, `Ctrl+H`, `Ctrl+U`, `Ctrl+T`), parameter usage examples, and Enter-to-run / auto-fill.
  - Registered `/commands` in `SLASH_COMMANDS` autocomplete.
- **Enhanced System Prompt Autonomous Protocols (`src/agent/prompt.rs`)**:
  - Updated `DEFAULT_SYSTEM_PROMPT` with explicit instructions on how minicode autonomously triggers native tool workflows for planning, scaffolding, goal loops, reviews, and checkpoints.
- **Dual-Mode NDJSON-RPC Support (`src/main.rs`, `src/agent/types.rs`)**:
  - Added `StdinCommand::ListCommands` returning complete structured JSON command catalogs over stdout (`AgentEvent::CommandList`).
  - Added `StdinCommand::ExecuteCommand { command, args }` allowing external AI orchestrators to invoke slash commands and goals programmatically.
  - Added `AgentEvent::IntentRouted` emitted whenever an intent is recognized.
- **Integration Test Suite (`tests/integration_intent_routing.rs`)**:
  - 10 comprehensive tests covering direct slash commands, natural language intent patterns, planning query extraction, catalog registry integrity, fuzzy filtering, and NDJSON-RPC command serialization.

## [0.0.62] — 2026-08-29

### Session I/O Performance, Event Schema & Quality Polish

- **Streaming I/O Performance (200x Speedup, `src/session/store.rs`)**:
  - Removed per-event `file.sync_all()` from `append_event()`. Avoids ~200 blocking filesystem sync syscalls per turn during LLM streaming.
- **First-Class UserPrompt Event (`AgentEvent::UserPrompt`, `src/agent/types.rs`, `src/agent/loop.rs`)**:
  - Added dedicated `AgentEvent::UserPrompt { turn_id, timestamp, prompt }` variant emitted and persisted at the beginning of each turn.
  - Aligns with OpenAI, Codex CLI, Aider, and Claude Code schemas for accurate session replays and Markdown exports.
- **Single-Pass Session & Metadata Loading (`src/session/store.rs`)**:
  - Added `load_session_with_metadata()` returning both metadata and events in a single file read, eliminating redundant file reopening in `get_session_summary_with_events()`.
- **Display-Width-Aware Truncation (`truncate_display`, `src/session/store.rs`, `src/ui/modal.rs`)**:
  - Introduced `truncate_display(s, max_cols, suffix)` utilizing `unicode-width` to properly handle full-width CJK characters and emojis in TUI column layouts.
- **Centralized Session Constants (`src/constants.rs`)**:
  - Extracted magic numbers across session storage and modal rendering into named constants (`SESSION_PREVIEW_MAX_BYTES`, `SESSION_FIRST_PROMPT_MAX_BYTES`, `SESSION_ID_DISPLAY_COLS`, `SESSION_LOAD_LIST_MAX`, etc.).
- **Slash Command Prefix Hardening (`src/app.rs`)**:
  - Tightened `/save` command prefix matching to `== "/save" || starts_with("/save ")`, preventing accidental prefix capture on prompts like `/savefile` or `/saving`.
- **Expanded Integration Tests (`tests/integration_session_history.rs`)**:
  - Added `test_user_prompt_in_summary_and_export` verifying user prompt extraction and Markdown timeline rendering (9 tests total in suite).

## [0.0.61] — 2026-08-28

### Session History Hardening & Quality Improvements

- **UTF-8 Slicing Panic Protection (`truncate_safe`, `src/session/store.rs`, `src/ui/modal.rs`)**:
  - Replaced unsafe byte slicing with `floor_char_boundary`-based `truncate_safe` across session export, commit rendering, and TUI response previews, preventing runtime panics on emojis, CJK text, and multi-byte unicode.
- **SessionStore Hardening & Security (`src/session/store.rs`, `src/error.rs`)**:
  - Added path traversal protection (`validate_session_id`) rejecting malicious paths containing `..`, `/`, `\`, or null bytes with `SessionError::InvalidId`.
  - Fixed TOCTOU race condition in `delete_session()` via direct `remove_file` matching on `io::ErrorKind::NotFound`.
  - Optimized `fork_session()` with buffered batch writes (`BufWriter`) and single sync flush.
- **StreamDelta & AI Transcript Persistence (`src/agent/loop.rs`, `src/session/store.rs`)**:
  - Persisted `StreamDelta` events directly to session JSONL, ensuring exported transcripts and session summaries preserve all assistant thinking and responses.
  - Fixed `list_sessions_rich()` serde tag parsing (`turn_start` and `stream_delta`) to accurately extract model info and prompt previews.
- **UI & Slash Command Enhancements (`src/ui/modal.rs`, `src/app.rs`)**:
  - Added viewport windowing to `ModalState::SessionBrowser` so long session lists scroll smoothly without overflowing terminal bounds.
  - Fixed `/export` fallback behavior when no sessions exist, preventing confusing "current session not found" errors.
  - Scoped `/load` to the workspace session store (`.minicode/sessions/`) matching `/sessions` and `/history`.
  - Improved `d` (delete) key in SessionBrowser with immediate in-memory item removal and user status feedback.
  - Renamed duration metric to "Tool Execution Time" / "Tool Time" for clarity.
- **Expanded Test Suite (`tests/integration_session_history.rs`)**:
  - Added 4 new integration tests (8 total in suite, 31 total across 6 test suites) covering UTF-8 safe truncation, path traversal rejection, corrupted JSONL resilience, and empty session summaries.

## [0.0.60] — 2026-08-28

### Interactive Visual Session History & Time-Travel Explorer

- **2-Column Visual History & Analytical Inspector (`ModalState::SessionBrowser`, `/history`, `Ctrl+H`)**:
  - Left pane (36% width): list of past sessions with active selection badge, relative time, event count, and session ID.
  - Right pane (64% width): live analytical preview showing model, turns, token metrics, duration, tool call distribution, touched files list, and prompt/response excerpts.
  - Hotkeys: <kbd>Enter</kbd> load, <kbd>f</kbd> fork branch, <kbd>e</kbd> export Markdown, <kbd>d</kbd> delete, <kbd>j</kbd>/<kbd>k</kbd> scroll.
- **SessionStore Analytical & Time-Travel Manipulation Engine (`src/session/store.rs`)**:
  - `SessionSummary`: comprehensive metrics computation across all turns and tools.
  - `get_session_summary()`: extracts session telemetry from recorded events.
  - `fork_session()`: forks existing session trajectory into a new branch without mutating historical recordings.
  - `export_markdown()`: compiles session trajectory into GitHub-Flavored Markdown transcripts.
  - `delete_session()`: safely deletes session files.
- **Dual-Mode CLI Subcommands (`src/main.rs`)**:
  - `minicode history [--json]`: human-readable session summary table or machine-readable JSON array.
  - `minicode export [session_id] [-o output.md]`: exports session transcripts directly from the command line.
- **Slash Commands & Shortcuts (`src/ui/input.rs`, `src/app.rs`)**:
  - Added `/history` and `/export` slash commands with full autocomplete.
  - Bound <kbd>Ctrl+H</kbd> for instant access to session history from anywhere in the TUI.
- **Integration Tests**:
  - Added 4 tests in `tests/integration_session_history.rs` verifying summaries, forking isolation, markdown exports, and deletion (27/27 total across all 6 test suites).

## [0.0.59] — 2026-08-28

### Browser Engines

- **Engine priority reordered: Obscura → Chrome → Firefox** (headless) and
  **Chrome → Obscura → Firefox** (GUI). Mozilla removed CDP in Firefox 141,
  so the Firefox fallback now only works with Firefox ≤ 140 ESR and the
  `remote.active-protocols` preference (auto-written to the profile).
- New `MINICODE_BROWSER=obscura|firefox|chrome` environment variable forces a
  specific engine, bypassing the automatic priority chain.

### Approval Enforcement (Security) (`src/agent/loop.rs`, `src/app.rs`, `src/main.rs`)

- **Real tool-approval gating**: `AgentLoop` now pauses before dispatching dangerous tools
  (`write_file`, `patch_file`, `exec_cmd`) when running in strict mode, emits the existing
  `approval_request` NDJSON event, and awaits a decision through a new shared
  `ApprovalRegistry` (`tokio::sync::oneshot` keyed by `tool_id`).
- **TUI wiring**: the approval modal's Accept/Esc-Reject now actually resolve the pending gate;
  "Allow Session" propagates the updated config to the agent actor; auto-approve skips the modal.
- **NDJSON protocol**: `tool_response` decisions (`approve`/`reject`) are routed to the pending
  gate by `tool_id`; stdin stays readable while turns block on approval.
- **Headless policy change**: one-shot mode refuses destructive tools with guidance unless
  `--yes` / `MINICODE_AUTO_APPROVE` is set.
- Rejected/cancelled tools produce paired failed tool results so LLM message history never
  contains unmatched function calls.

### Fixed

- **NDJSON command parsing** (`src/main.rs`): stdin commands are parsed via buffered
  `serde_json::Value` → `from_value`; the adjacent-tag streaming deserializer mis-reported
  "missing field `params`" for valid input. Bare `{"method":"abort"}` is also tolerated.
- **Cancelled turns** now emit `turn_end.status = "cancelled"` (was `"complete"`), keep
  tool-call/result pairing, clean up registry entries, and roll back partial completion-token
  counts on retry.
- **SSRF hardening** (`src/tools/web.rs`): literal-IP check now handles bracketed IPv6 and
  IPv4-mapped forms (`[::ffff:10.0.0.5]`); hostnames are re-checked after DNS resolution to
  defeat rebinding hosts (e.g. `localtest.me` → 127.0.0.1).
- Stale `ToolPipeline` integration test updated for the `file_before` parameter; stale
  tool-schema count assertion fixed (51 → 57).

### Changed

- `git.dirty_commit` and `git.ai_commit_messages` config options are now honored:
  `dirty_commit = true` stages all workspace changes (`git add -A`);
  `ai_commit_messages = false` produces plain commit messages instead of conventional ones.
## [0.0.59] — 2026-08-28

### Dual-Mode CLI Interoperability for Humans, Scripts & External AI Agents

- **Native CLI Subcommands (`minicode stack`, `minicode diff`, `minicode review`, `minicode doctor`, `minicode sync`, `minicode plan`)**:
  - `minicode stack list [--json]`: Lists all built-in & custom architecture stacks with runtime, package counts, and descriptions.
  - `minicode stack show <name> [--json]`: Displays full file layout tree and dependency list for a specific stack.
  - `minicode stack add <name> [--dir <path>] [--no-install]`: Direct CLI scaffolding of stacks into projects.
  - `minicode diff [--staged] [--json]`: Prints syntax-colored unified diff or structured JSON diff for CI/scripts/agents.
  - `minicode review [--staged] [--json]`: Executes 5-pillar adversarial code review directly from CLI with scorecards or JSON payload.
  - `minicode doctor`: Runs multi-runtime health checks and diagnostics from terminal.
  - `minicode sync`: Synchronizes `onpkg.json` manifest and `AGENTS.md` docs.
  - `minicode plan [prompt]`: Runs autonomous milestone planning non-interactively.
- **Machine-Readable Dual-Mode Protocols**:
  - Derived `serde::Serialize` and `serde::Deserialize` on `GitDiffFile`, `GitDiffLine`, `ReviewReport`, and `ReviewFinding`.
  - All tools and review pipelines are 100% interoperable across Ratatui TUI, Headless NDJSON streaming (`--json-stream`), MCP stdio server (`minicode serve`), and direct CLI subcommands.

## [0.0.58] — 2026-08-28

### Interactive TUI Git Diff Viewer & Multi-Agent Code Reviewer (`/diff`, `/review`, `Ctrl+D`)

- **Interactive TUI Git Diff Viewer (`ModalState::GitDiff`, `/diff`, `Ctrl+D`)**:
  - Implemented visual 2-column git diff modal in Ratatui TUI with live file tree and syntax-highlighted unified diff preview.
  - Line numbers (`old │ new`), hunk headers (`@@ ... @@`), and additions (`+`) / deletions (`-`) with theme styling.
  - Interactive file list with change indicators (`[M]`, `[A]`, `[D]`, `[?]`) and additions/deletions counts (`+X -Y`).
  - Hotkey actions:
    - <kbd>Tab</kbd>: Toggle between Staged (`--cached`) and Working Tree (`unstaged`) views.
    - <kbd>s</kbd>: Stage / Unstage current file (`git add` / `git restore --staged`).
    - <kbd>j</kbd> / <kbd>k</kbd> & <kbd>PgUp</kbd> / <kbd>PgDn</kbd>: Scroll diff preview.
    - <kbd>r</kbd>: Trigger multi-agent code review directly on the diff.
    - <kbd>Esc</kbd> / <kbd>q</kbd>: Close modal.
- **Multi-Agent Adversarial Code Reviewer (`GitReviewer`, `/review`)**:
  - Multi-dimensional audit analyzing git changes across 5 critical dimensions:
    1. **Security & Secret Leakage** (hardcoded keys, credential patterns, token assignments)
    2. **Correctness & Invariant Safety** (explicit `.unwrap()`, `.expect()` panics)
    3. **Architecture & Observability** (stdout `println!` in library code)
    4. **Verification & Test Coverage** (code additions without associated test blocks)
    5. **Summary & Scorecard** (0-100 quality score, finding counts, line-by-line snippets)
  - Registered `git_review` tool in `ToolRegistry` and `/review` slash command in `InputDock`.
- **Verification**:
  - 5 integration tests in `tests/integration_git_diff_review.rs`.
  - 23/23 total integration tests passing across all 5 test suites (100%).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.57] — 2026-08-28

### Interactive TUI Stack Wizard & Autonomous Goal/Plan Slash Commands (`/stack`, `/plan`, `/goal`)

- **Interactive TUI Stack Wizard (`ModalState::StackSelect`, `/stack`)**:
  - Implemented visual 2-column modal in Ratatui TUI with live search filtering across all embedded and custom stacks.
  - Live preview panel displaying runtime badges, tech tags, descriptions, package dependencies, and file layout trees.
  - One-click native scaffolding with `Enter` and real-time timeline status reporting.
- **Autonomous Goal & Planning Slash Commands (`/plan`, `/goal`)**:
  - `/plan [prompt]`: Automatically analyzes workspace architecture, breaks work into verifiable milestones, and maintains `onpkg_docs/todo.md` and `onpkg_docs/implementation.md`.
  - `/goal [prompt]`: Initiates self-directed autonomous execution loop with automated tests and verification checkpoints until all tasks are marked complete (`[x]`).
- **Autonomous Intent Protocols in System Prompt (`src/agent/prompt.rs`)**:
  - Enhanced `DEFAULT_SYSTEM_PROMPT` to automatically map user natural language requests to native tools (`onpkg_stack_list`, `onpkg_stack_add`, `semantic_search`, `locate_symbol`, `onpkg_sync`).
- **Slash Commands Autocomplete Registry (`src/ui/input.rs`)**:
  - Registered `/stack`, `/plan`, `/goal` with instant fuzzy autocomplete recommendations in `InputDock`.
- **Verification**:
  - 8 integration tests in `tests/integration_onpkg_tools.rs`.
  - 18/18 total integration tests passing (100%).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.56] — 2026-08-28

### Embedded Native onpkg Template Engine & Autonomous Spec Syncer (`src/tools/onpkg/`)

- **100% Embedded Pure-Rust Stack Templates (`src/tools/onpkg/templates/builtin/`)**:
  - Embedded full production architectures directly into binary memory (`react-vite`, `react-vite-full`, `react-vite-gsap`, `next-template`, `fastapi`, `flutter-riverpod-my_app`, `hono-api`, `hono-full`, `mern`, `pern`).
  - Zero external CLI dependency required — standalone single-binary scaffolding anywhere.
- **Native Multi-Runtime Package Manager & Auto-Installer (`OnpkgScaffolder`)**:
  - Automatically writes all stack files, generates `onpkg.json` and `AGENTS.md`, creates `onpkg_docs/` spec suites (`prd.md`, `design.md`, `implementation.md`, `todo.md`), and executes runtime-optimized package managers (`bun`, `uv`, `cargo`, `flutter`).
- **Autonomous Project Sync & Diagnostics (`OnpkgSyncEngine`, `OnpkgDoctor`)**:
  - Real-time project runtime detection and manifest synchronization.
  - Multi-runtime environment doctor inspecting Bun, UV, Node, Python, Flutter, Cargo, and Git.
- **Native Skills Manager (`OnpkgSkillsManager`)**:
  - Discovers and installs active agent skills across `.minicode/skills/`, `onpkg_docs/`, and global directories.
- **Verification**:
  - 5 integration tests in `tests/integration_onpkg_tools.rs` (100% pass).
  - 15/15 tests across all suites passing.
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.55] — 2026-08-28

### Native onpkg Tool Suite & Multi-Runtime Stack Scaffolder (`src/tools/onpkg/`, `onpkg_tools.rs`)

- **High-Performance `onpkg` Client Engine (`OnpkgClient`)**:
  - Automatically discovers `onpkg` binary via `which` and standard system paths (`~/.cargo/bin/onpkg`, `~/.local/bin/onpkg`, `/usr/local/bin/onpkg`).
  - Executes commands with structured JSON output parsing, rich human-readable fallback, and graceful error diagnostics.
- **7 New Native `onpkg` Tools & Schemas (`src/tools/registry/onpkg_tools.rs`)**:
  - `onpkg_stack_list`: Query and list all built-in and custom application stacks with categories, technology tags, and file counts.
  - `onpkg_stack_show`: Inspect the detailed manifest, package dependencies, and file structure of any stack.
  - `onpkg_stack_add`: Instantly scaffold complete, production-grade application stacks into the target folder with automatic online dependency installation (`bun`, `uv`, `cargo`, `flutter`) and `onpkg_docs/` / `AGENTS.md` generation.
  - `onpkg_skill_list`: Discover all installed and available domain-specific AI agent skills.
  - `onpkg_skill_install`: Install specialized markdown skills into `.minicode/skills/` or `onpkg_docs/`.
  - `onpkg_sync`: Recursively scan project files and dependencies to update `onpkg.json`, synchronize `AGENTS.md`, and refresh spec-driven workflow documents (`prd.md`, `design.md`, `todo.md`).
  - `onpkg_doctor`: Run multi-runtime environment health diagnostics (Bun, Node.js, UV/Python, Cargo, Flutter).
- **Verification**:
  - 4 integration tests in `tests/integration_onpkg_tools.rs` (100% pass).
  - 14/14 tests across all suites passing.
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.54] — 2026-08-25

### Native GitHub Integration & CI Workflow Diagnoser (`src/tools/github/`, `git_tools.rs`)

- **Hybrid `gh` CLI + REST API Engine (`GitHubClient`)**:
  - Auto-detects local `gh` CLI credentials for zero-friction authentication without token management.
  - Seamlessly falls back to `GITHUB_TOKEN` / `GH_TOKEN` for REST API endpoints.
  - Auto-detects GitHub repository owner/slug from `git remote origin`.
- **Closed-Loop Issue & Pull Request Toolkit (`GitHubService`)**:
  - `github_issue_view`: Read issue description, labels, and discussion thread comments.
  - `github_issue_list`: Search and list open/closed repository issues.
  - `github_issue_create`: Create structured issues with titles, markdown descriptions, and labels.
  - `github_pr_view`: View PR details, target branches, and addition/deletion statistics.
  - `github_pr_diff`: Fetch raw or unified diffs for any pull request.
  - `github_pr_create`: Open a pull request from the current branch against base with conventional summary.
- **GitHub Actions CI Failure Diagnoser (`GitHubService`)**:
  - `github_ci_status`: Query GitHub Actions workflow run statuses (success, failure, in_progress).
  - `github_ci_logs`: Pull and compact failed job and step error logs for automatic build break remediation.
- **Verification**:
  - 3 integration tests in `tests/integration_github_tools.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.53] — 2026-08-25

### Deep Recursive Web Crawler & Documentation Ingestion Engine (`src/tools/crawler/`, `web_tools.rs`)

- **Bounded BFS Crawler Engine (`CrawlerEngine`)**:
  - Recursively crawls documentation sites with strict domain boundaries (`same_origin` / `path_prefix`), depth limits, and max page caps.
  - Controls concurrency via `tokio::sync::Semaphore` (4 parallel fetches) with anti-loop visited deduplication.
- **Sitemap XML & `llms.txt` Auto-Discovery (`SitemapParser`)**:
  - Automatically probes and parses `/sitemap.xml`, `/sitemap_index.xml`, and `/llms.txt` / `/llms-full.txt` endpoints to seed documentation frontiers.
- **Fit-Markdown Boilerplate Distillation (`MarkdownDistiller`)**:
  - Strips noisy navigation sidebars, footers, headers, ads, and cookie banners to extract high-density GFM Markdown, saving 80–90% of tokens.
  - Automatically caches crawled documentation to `.minicode/crawled/` for sub-second offline recall.
- **New Web Tools & Schemas (`src/tools/registry/web_tools.rs`)**:
  - `crawl_documentation`: Recursively crawl and distill documentation into a structured report.
  - `crawl_sitemap`: Auto-extract documentation routes from XML sitemaps.
  - `search_crawled_docs`: Search cached documentation locally in workspace.
- **Verification**:
  - 4 integration tests in `tests/integration_doc_crawler.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.52] — 2026-08-25

### Subagent Shared Scratchpad & Inter-Worker Messaging Bus (`src/agent/subagent/scratchpad.rs`, `agent_tools.rs`)

- **Shared Scratchpad Blackboard (`SharedScratchpad`, `ScratchpadEntry`)**:
  - Thread-safe key-value knowledge repository allowing subagent workers and orchestrators to publish intermediate findings, test failure traces, and API signatures without inflating conversational prompt context.
  - Automatically persists to `.minicode/scratchpad.json`.
- **Inter-Worker Messaging Bus (`WorkerMessageBus`, `WorkerMessage`)**:
  - Asynchronous publish/subscribe message broker enabling direct worker-to-worker communication and swarm-wide broadcasts.
  - Workers can post and inspect targeted task inboxes.
- **New Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**:
  - `scratchpad_write`: Publish or update an entry on the shared blackboard.
  - `scratchpad_read`: Query an entry by key.
  - `scratchpad_list`: Inspect all active findings on the blackboard.
  - `send_worker_message`: Dispatch direct or broadcast messages across worker swarm.
  - `read_worker_messages`: Fetch pending inbox messages for a worker.
- **Verification**:
  - 3 integration tests in `tests/integration_scratchpad_bus.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.51] — 2026-08-25

### Semantic AST Code-Chunk Semantic Embedder & Symbol Indexer (`src/context/semantic.rs`, `search_tools.rs`)

- **Tree-sitter AST Symbol Chunking (`chunk_source_code_ast`)**:
  - Replaces rigid line-window chunking with AST-aware symbol boundary chunking (`functions`, `structs`, `classes`, `traits`, `enums`, `interfaces`).
  - Seamlessly falls back to sliding line chunking for plain text, Markdown, or non-AST files.
- **Symbol Name & Signature Embedding Boost (`SemanticIndex::search_symbols`)**:
  - Boosts symbol names and type signatures during vector embedding to drastically improve direct function and type recall.
- **New Search Tool & Schemas (`src/tools/registry/search_tools.rs`)**:
  - `search_symbols_semantic`: Semantically search specifically for AST symbol definitions matching concepts or function intent.
- **Verification**:
  - 2 integration tests in `tests/integration_ast_semantic_indexer.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.50] — 2026-08-25

### Milestone v0.0.50: Resilient Stream Re-Connection & Network Circuit Breaker (`src/agent/circuit_breaker.rs`, `provider.rs`)

- **Network Circuit Breaker (`CircuitBreaker`, `CircuitState`)**:
  - Implements three-state circuit breaker protection (`Closed`, `Open`, `HalfOpen`) for all upstream LLM provider API streams.
  - Automatically trips to `Open` upon reaching failure threshold (default: 3 network/API errors), fast-failing subsequent requests to prevent API hammering.
  - Transitions to `HalfOpen` after cooldown (10s) and automatically recovers to `Closed` on consecutive successful canary streams.
- **Exponential Backoff & Retry Policy (`RetryPolicy`)**:
  - Automatically retries transient network errors (HTTP 429 rate limits, 502/503/504 server errors, connection resets) with jittered exponential backoff.
  - Prevents retrying non-transient errors (400 Bad Request, 401 Unauthorized, 403 Forbidden).
- **Resilient Provider Wrapper (`ResilientProvider<P>`)**:
  - Transparently decorates any `Provider` implementation with circuit breaker protection and auto-retry logic.
- **Verification**:
  - 3 integration tests in `tests/integration_resilient_network.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.49] — 2026-08-24

### Speculative Multi-Branch Hypothesis Auto-Pruner & Parallel Evaluator (`src/agent/hypothesis.rs`, `agent_tools.rs`)

- **Parallel Multi-Branch Evaluation (`HypothesisEngine::evaluate_all_branches`)**:
  - Concurrently evaluates all active speculative hypothesis worktree branches using fast compiler diagnostics.
- **Auto-Pruner Engine (`HypothesisEngine::prune_failed_branches`)**:
  - Automatically discards underperforming or failing branches below a fitness score threshold, purging temporary git worktrees.
- **Comparison Matrix Markdown Formatter (`HypothesisEngine::format_comparison_matrix`)**:
  - Generates a structured comparison matrix comparing branch ID, status, fitness score, compiler errors/warnings, and approach descriptions.
- **New Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**:
  - `evaluate_all_branches`: Batch evaluation of all speculative branches.
  - `prune_branches`: Automated cleanup of low-fitness worktrees.
  - `compare_branches`: Table visualization of alternative implementation trade-offs.
- **Verification**:
  - 2 integration tests in `tests/integration_hypothesis_pruner.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.48] — 2026-08-24

### Episodic Vector Memory & Long-Term Recall Engine (`src/context/episodic.rs`, `agent_tools.rs`)

- **Episodic Knowledge Recording (`src/context/episodic.rs`)**:
  - `EpisodicItem`: Stores session breakthroughs, bug root causes, architectural patterns, and code references.
  - Automatically embeds episode text into 128-dimensional dense vectors using `SemanticIndex::embed`.
- **Hybrid Semantic + Keyword Search (`EpisodicMemory::search`)**:
  - Blends cosine vector similarity (0.7) and keyword token overlap (0.3) for resilient knowledge retrieval.
- **Workspace State Persistence (`EpisodicMemory::save`, `EpisodicMemory::load`)**:
  - Automatically persists episodic memory across sessions to `.minicode/episodic_memory.json`.
- **New Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**:
  - `record_episode`: Saves a completed task episode with tags and code references.
  - `recall_episodes`: Queries past historical solutions to guide future reasoning.
- **Verification**:
  - 3 integration tests in `tests/integration_episodic_memory.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.47] — 2026-08-24

### RTK-Style Token Output Compactor & Multi-Language Diagnostic Distiller (`src/tools/compactor.rs`)

- **Multi-Language Test Compaction (`src/tools/compactor.rs`)**:
  - `compact_pytest`: Distills verbose `pytest` / `python -m unittest` output to one-line summaries on pass and isolates `FAILURES` traceback frames on failure.
  - `compact_go_test`: Distills `go test` output into clean pass packages or failing test assertion traces (`--- FAIL: `).
  - `compact_cargo_test` & `compact_cargo_check`: Eliminates downloading/compiling noise and extracts exact diagnostic locations and `error[...]` blocks.
- **Compaction Metrics & Token Savings Tracker**:
  - `calculate_compaction_stats`: Computes raw bytes vs compacted bytes and percentage savings (achieving 85–95% token savings on verbose tool outputs).
- **Fast Diff Folding Engine**:
  - Automatically folds large diff hunks while preserving context boundaries and header signatures.
- **Verification**:
  - 4 integration tests in `tests/integration_token_compactor.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.46] — 2026-08-24

### Task DAG & Dynamic Dependency Graph Engine (`src/agent/task_dag.rs`, `agent_tools.rs`)

- **Parallel Execution Wave Calculation (`TaskDag::calculate_execution_waves`)**:
  - Automatically computes topologically-sorted non-conflicting parallel execution waves using Petgraph DiGraphs.
  - Allows orchestrators and agents to schedule independent subtasks concurrently in parallel worktrees.
- **Dynamic Task Splitting & Graph Mutation (`TaskDag::split_task`)**:
  - Breaks high-complexity tasks (complexity > 7) into finer-grained child tasks during execution.
  - Seamlessly re-wires upstream and downstream DAG dependencies to the new child task sequence.
- **Workspace State Persistence (`TaskDag::save`, `TaskDag::load`)**:
  - Automatically saves and loads task state to `.minicode/task_dag.json`.
- **New Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**:
  - `schedule_task_waves`: Outputs ordered wave stages with progress badges and task complexity.
  - `split_task`: Dynamically partitions an active task into subtasks and updates the DAG in place.
- **Verification**:
  - 3 integration tests in `tests/integration_task_dag_waves.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.45] — 2026-08-24

### Adaptive Inline Subagent UI & Swarm Live Stream Engine (`src/ui/view.rs`, `src/ui/theme.rs`, `src/ui/status.rs`)

- **Crush & OpenCode Style Inline Tree Hierarchy (`src/ui/view.rs`)**:
  - `TimelineEntry::SubagentTree`: Renders inline subagent trees for 1–3 workers with distinct filled role ` Task ` badges, status bullets (`◉`, `✔`, `✗`), and live tree action branches (`├─── `, `╰─── `).
  - Multi-line indented task prompts and dedicated `Outcome: ` / `Error: ` summary blocks beneath tree branches.
- **Adaptive Swarm Matrix Card (`TimelineEntry::SubagentSwarm`)**:
  - Automatically condenses 4+ parallel subagents into a unified, clean inline matrix card with active worker counts, token metrics, and runtime stats.
  - Interactive expand/collapse toggle (`toggle_subagent_swarm`) to switch between compact overview and full tree detail.
- **Custom Theme & Role Palettes (`src/ui/theme.rs`)**:
  - `Theme::role_accent_color(role)`: Maps Researcher (Cyan/Aqua), CodeReviewer (Magenta/Pink), TestEngineer (Green/Mint), SecurityAuditor (Warm Orange/Yellow), and Worktree/Custom (Brand Purple/Blue).
- **Subagent Swarm Statusline Indicator (`src/ui/status.rs`)**:
  - Renders `swarm:N workers` in the bottom statusbar when background subagents are registered.
- **Verification**:
  - 3 integration tests in `tests/integration_subagent_ui.rs` (100% pass).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.44] — 2026-08-24

### Actor-Critic Dual-Agent Code Verification Loop (`src/agent/critic.rs`, `agent_tools.rs`)

- **Multi-Dimensional Critic Engine (`src/agent/critic.rs`)**:
  - Compiler Diagnostics via `FastCompilerChecker` (maps errors/warnings to exact file paths and lines).
  - Anti-Pattern and Logging Scanner (detects raw `println!`/`eprintln!` in non-test library code).
  - Zero-Leak Secret Scanner via `SecretRedactor` (detects hardcoded API keys and tokens).
  - Structured `CriticVerdict` (`Approved`, `ApprovedWithWarnings`, `Rejected`) and markdown formatting.
- **Enhanced Agent Tool (`src/tools/registry/agent_tools.rs`)**:
  - Formats multi-axis report in `critic_review` dispatch.
- **Verification**:
  - 4 integration tests in `tests/integration_actor_critic.rs` (100% pass).

## [0.0.43] — 2026-08-24

### Subagent Swarm Core Engine & Capability Sandboxing (`src/agent/subagent/`)

- **Specialized Role Presets & Capability Sandboxing (`src/agent/subagent/types.rs`)**:
  - `Researcher`: Read-only codebase explorer and web documentation researcher (strict ban on `write_file`, `patch_file`, and `exec_cmd`).
  - `CodeReviewer`: Multi-axis code reviewer inspecting diffs, AST contracts, and standards.
  - `TestEngineer`: Test runner with capability to execute test commands and write test suites under `tests/`.
  - `SecurityAuditor`: Audit scanner for secret leaks, injection vectors, and permission boundaries.
  - `Custom`: User-defined role with custom prompt, token limits, and tools.
- **Isolated Token Budgeting & Worker Loop (`src/agent/subagent/worker.rs`)**:
  - Runs in an isolated async task maintaining private message history (preventing context window inflation for parent agent).
  - Uses `tiktoken-rs` to track token consumption against strict role budgets (e.g. 24k tokens for Researcher, 16k tokens for Reviewer).
  - Enforces tool capability whitelisting on every turn.
- **Supervisor Pool & Lifecycle Manager (`src/agent/subagent/pool.rs`)**:
  - `SubagentPool`: Thread-safe worker registry tracking active states (`Idle`, `Running`, `Completed`, `Failed`, `Canceled`).
  - Live status formatting and graceful task cancellation via `kill_subagent` and `kill_all`.
- **New Agent Tools (`src/tools/registry/agent_tools.rs`)**:
  - `invoke_subagent`: `{ role: "researcher" | "code_reviewer" | "test_engineer" | "security_auditor" | "custom", prompt: string, model?: string, token_budget?: integer, max_turns?: integer }`
  - `send_message`: `{ subagent_id: string, message: string }`
  - `manage_subagents`: `{ action: "list" | "status" | "kill" | "kill_all", subagent_id?: string }`
- **Verification**:
  - 6 integration tests in `tests/integration_subagent_swarm.rs` + existing worktree test in `tests/integration_subagents.rs`.
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.42] — 2026-08-23

### Fit Markdown & Intelligent Documentation Ingestion (`src/tools/browser/markdown.rs`)

- **3-Step Smart Markdown Pipeline (`src/tools/browser/markdown.rs`)**:
  - **Step 1: Content Negotiation (`Accept: text/markdown`)**: Requests Markdown directly from modern developer APIs and docs servers (bypassing heavy HTML generation).
  - **Step 2: `llms.txt` & `llms-full.txt` Probing**: Automatically probes root and parent endpoints (`/llms.txt`, `/.well-known/llms.txt`) for token-optimized clean text summaries.
  - **Step 3: Fast HTML-to-Fit-Markdown Distillation**: Uses `htmd` converter with noise pruning, stripping scripts, stylesheets, tracking code, navbars, and footers to reduce LLM context token usage by 70–90%.
- **Targeted Query Filtering**:
  - `SmartMarkdownExtractor::extract_fit_markdown(html, query)`: filters extracted documentation paragraphs by relevant search keywords while preserving document hierarchy and code blocks.
- **Upgraded `fetch_or_browse` Agent Tool (`src/tools/web.rs`, `web_tools.rs`)**:
  - Routes web documentation requests through the smart 3-step pipeline while strictly enforcing SSRF security validation.
  - Added optional `query` parameter to `fetch_or_browse` schema: `{ url: string, query?: string }`.
- **Verification**:
  - 4 integration tests in `tests/integration_browser_markdown.rs` (18 total browser integration tests passing).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.41] — 2026-08-23

### Interactive Browser Automation & Live Dev-Server Debugger (`src/tools/browser/`)

- **Versioned Accessibility Tree & Stale Reference Guards (`src/tools/browser/accessibility.rs`)**:
  - `AccessibilityManager`: assigns versioned `@v{rev}:e{idx}` references to all interactive elements in DOM source order.
  - Automatically increments revision counter across state-changing actions and page navigations.
  - Rejects stale element references (e.g. attempting to click `@v1:e2` after the page mutated to revision `v2`) with actionable feedback.
  - Supports both exact (`@v1:e1`) and shorthand (`@e1`) reference resolution.
- **Interactive Actions with Immediate Snapshot Return (`src/tools/browser/interaction.rs`)**:
  - `BrowserInteractor::click_element()`: scrolls element into view, dispatches click events, waits for DOM settling, and returns the **updated ARIA tree snapshot in the same turn** (saving an entire LLM roundtrip).
  - `BrowserInteractor::fill_element()`: focuses inputs/textareas, types text, fires synthetic `input` and `change` events, and returns the updated ARIA tree snapshot.
  - `BrowserInteractor::scroll_page()`: scrolls the active viewport (`up`, `down`, `top`, `bottom`).
- **Live Diagnostics Collector (`src/tools/browser/debug.rs`)**:
  - `DebugCollector`: captures browser console logs (`INFO`, `WARN`, `ERROR`), uncaught runtime exceptions, and failed HTTP requests (`4xx`/`5xx`).
  - `format_report()`: generates chronological markdown reports ideal for inspecting localhost dev servers (`http://localhost:3000`).
- **Agent Tool Registry (`src/tools/registry/web_tools.rs`)**:
  - `browser_click`: `{ ref: "@v1:e1", mode?: "headless" | "gui" }`
  - `browser_fill`: `{ ref: "@v1:e2", text: "...", mode?: "headless" | "gui" }`
  - `browser_scroll`: `{ direction: "down", mode?: "headless" | "gui" }`
  - `browser_debug_logs`: `{ mode?: "headless" | "gui" }`
- **Verification**:
  - 5 integration tests in `tests/integration_browser_interaction.rs` and 9 tests in `tests/integration_browser_engine.rs` (14 total browser integration tests passing).
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted.

## [0.0.40] — 2026-08-23

### Dual-Mode Browser Engine Core & Multi-Browser Manager (`src/tools/browser/`)

- **Multi-Engine Priority Chains (`src/tools/browser/engine.rs`)**:
  - `HEADLESS_PRIORITY`: `Obscura` (Pure-Rust V8 engine, ~30MB RAM, <85ms boot) $\rightarrow$ `Firefox Headless` (Gecko) $\rightarrow$ `Chrome Headless` (Chromium).
  - `GUI_PRIORITY`: `Firefox GUI` (visible window) $\rightarrow$ `Chrome GUI` (fallback visible window).
  - Strongly typed `BrowserEngine`, `BrowserMode`, and `EngineConfig` structs with mode string parsing.
- **Process Supervisor & Isolated Profile Sandbox (`src/tools/browser/manager.rs`)**:
  - Runtime binary discovery via `which` crate across system PATH for candidate executables.
  - Dedicated profile isolation under `.minicode/browser_profiles/<engine>_<mode>/` preventing desktop browser instance collision and process hijacking.
  - Generates engine-specific arguments (`--stealth`, `--allow-file-access`, `--remote-debugging-port`, `--headless=new`, `--user-data-dir`).
  - Child process spawning with `kill_on_drop(true)` and Unix process group isolation (`process_group(0)`).
  - Port readiness polling against `http://127.0.0.1:{port}/json/version`.
- **Page-Target CDP WebSocket Driver (`src/tools/browser/driver.rs`)**:
  - Lightweight async CDP client using `tokio-tungstenite` over raw WebSocket frames.
  - Resolves direct Page Target WebSocket URLs via `GET /json/list` / `PUT /json/new`.
  - Automatic JavaScript dialog dismissal for `Page.javascriptDialogOpening` preventing automation lockups.
  - Core CDP capabilities: `navigate(url)`, `get_document_html()`, `evaluate_js(expr)`, `take_screenshot()`.
- **Public Controller Facade & SSRF Security (`src/tools/browser/mod.rs`)**:
  - Migrated and structured `PageSnapshot` and `AriaElement` models.
  - Controlled SSRF policy (`BROWSER_BLOCKED_HOSTS`) allowing loopback/`localhost` testing on dev servers while blocking cloud metadata endpoints (`169.254.169.254`).
  - Graceful zero-browser HTTP fallback for CI environments without local browser binaries.
- **Agent Tool Registry (`src/tools/registry/web_tools.rs`)**:
  - `browser_navigate`: accepts optional `mode: "headless" | "gui"`.
  - `browser_snapshot`: extracts ARIA accessibility tree with numbered versioned references (`@v1:e1`).
  - `browser_eval`: evaluates arbitrary JavaScript in the active browser context.
  - `browser_screenshot`: captures viewport PNGs and saves them to `.minicode/screenshots/`.
- **Verification**:
  - 9 integration tests in `tests/integration_browser_engine.rs` covering engine priorities, mode parsing, launch arguments, ARIA parsing, URL validation, and tool schemas.
  - 0 clippy warnings (`cargo clippy -- -D warnings`), 100% formatted (`cargo fmt`).

## [0.0.39] — 2026-08-22

### Inline Diff Preview for File-Modifying Tools (`src/tools/diff.rs`)

- **Diff Engine (`src/tools/diff.rs`)**:
  - `compute_diff(old, new)` using the `similar` crate (`TextDiff::from_lines`).
  - `has_changes()` — returns `true` only when additions or deletions exist.
  - `format_diff_plain()` — produces unified-style diff string for export/session save.
  - Line cap at 60 diff lines to prevent timeline flood; trailing context trimmed.
- **`DiffMiddleware` (`src/tools/middleware.rs`)**:
  - 4th stage in the default pipeline: `Timing → Redact → Checkpoint → Diff`.
  - Snapshots file content **before** dispatch (in `loop.rs`), passes it via new `ToolContext::file_before` field.
  - Reads new file content from disk after successful execution and computes diff.
  - Embeds diff in tool output using `MINICODE_DIFF_BLOCK:` marker prefix — transparent to non-TUI consumers.
  - No-ops for failed tools, read-only tools, and unchanged files.
- **TUI Renderer (`src/ui/view.rs`)**:
  - `ToolFinished` block detects `MINICODE_DIFF_BLOCK:` prefix and renders diff section before regular output.
  - `---`/`+++` headers rendered in muted italic.
  - Addition lines (`+ line`) rendered in `theme.success` (green) with bold `+`.
  - Deletion lines (`- line`) rendered in `theme.destructive` (red) with bold `-`.
  - Context lines rendered in muted text.
  - Excess diff lines folded with count summary.
- **Verification & Testing**:
  - 5 unit tests in `src/tools/diff.rs` covering additions, deletions, no-change, new-file, and plain formatting.
  - 8 unit tests updated in `src/tools/middleware.rs` for new `file_before` field.
  - 11 integration tests in `tests/integration_inline_diff.rs`: diff engine, middleware skip conditions, real filesystem diff attachment, no-diff-when-unchanged, and pipeline signature.
  - All 200+ tests passing, 0 clippy warnings.

## [0.0.38] — 2026-08-22

### Tool Middleware Pipeline (`src/tools/middleware.rs`)

- **`ToolMiddleware` trait** (`src/tools/middleware.rs`):
  - Composable `after(&ctx, result) -> ToolResult` hook applied to every tool execution (built-in and MCP).
  - `ToolContext` carries `tool_name`, `workspace_root`, and raw JSON `args` for each call.
  - `ToolPipeline` runs middlewares in declaration order via `Iterator::fold`.
- **Three built-in middlewares**:
  - `TimingMiddleware` — emits `tracing::debug!` span with tool name, success, duration, and output size on every execution.
  - `RedactMiddleware` — wraps the existing `SecretRedactor::global().redact()` call; replaces the inline ad-hoc hook in `agent/loop.rs`.
  - `CheckpointMiddleware` — emits `tracing::info!` for every `write_file`/`patch_file` invocation with file path and result, providing an immutable audit trail.
- **`AgentLoop` integration**:
  - Added `tool_pipeline: ToolPipeline` field initialized with `ToolPipeline::default()`.
  - Replaced the four-line inline redaction block in `src/agent/loop.rs` with a single `self.tool_pipeline.run(...)` call.
- **Verification & Testing**:
  - 8 unit tests in `src/tools/middleware.rs` covering each middleware individually and composed pipeline.
  - 12 integration tests in `tests/integration_tool_middleware.rs` covering redaction correctness, identity transforms, metadata preservation, and failure flag propagation.
  - All 190+ tests passing, 0 clippy warnings.

## [0.0.37] — 2026-08-22

### Workspace-Local Session Storage & Interactive Session Browser (`/sessions`)

- **Workspace-Local Session Storage (`src/session/store.rs`)**:
  - Added `SessionStore::with_workspace(workspace_root)` constructor.
  - Sessions now stored in `<workspace>/.minicode/sessions/` when the `.minicode/` directory exists — scoped per project, exactly like Codex and Claude Code.
  - Graceful fallback to `~/.config/minicode/sessions/` when no `.minicode/` dir is present.
  - `AgentLoop::new()`, `--continue`, and `--resume` all switched to workspace-local store.
- **Session Metadata Enrichment (`src/session/store.rs`)**:
  - Added `event_count` and `preview` fields to `SessionMetadata` (zero-cost via `#[serde(default)]`).
  - Added `list_sessions_rich()` that reads JSONL files to populate event counts for the modal display.
- **Interactive Session Browser Modal (`/sessions`)**:
  - Type `/sessions` (or `/history`) to open a full-height TUI modal.
  - Shows each session with: relative time (`2h ago`, `3d ago`), shortened workspace path (`…/project/dir`), and event count.
  - Session ID shown below in muted style.
  - Navigate with `↑`/`↓`, `Enter` loads and hydrates the selected session into the timeline, `Esc` cancels.
  - Added `/sessions` to autocomplete slash command list and `/help` modal.
- **Verification & Testing**:
  - Added 7 integration tests in `tests/integration_session_browser.rs`: workspace-local path, global fallback, sorted list, rich event count, round-trip load, not-found error, and latest session ID.
  - 3 new unit tests in `src/session/store.rs`: `test_with_workspace_uses_minicode_dir`, `test_list_sessions_rich_returns_event_count`.
  - All tests passing with zero compiler or clippy warnings.

## [0.0.36] — 2026-08-21

### Zero-Leak Secret Redaction Proxy (`src/sandbox/redact.rs`)
- **Centralized Secret Redaction Engine**:
  - Created `SecretRedactor` with lazy-init `OnceLock` global instance.
  - 16 compiled regex patterns covering: OpenAI (`sk-...`, `sk-proj-...`), Anthropic (`sk-ant-...`), GitHub (`ghp_`, `ghs_`, `github_pat_`), AWS (`AKIA...`, secret key assignments), Google (`AIza...`), Stripe (`sk_live_`, `rk_test_`), Slack (`xox[bpras]-...`), Bearer tokens, JWT tokens, PEM private key blocks, generic `api_key=...` assignments, connection string passwords, and hex secrets.
  - Environment variable harvesting: scans host env at init for vars matching `SECRET_PATTERNS` and `BLOCKED_PREFIXES`, stores values for exact-match redaction.
- **Single Hook Architecture (`src/agent/loop.rs`)**:
  - Inserted redaction between tool dispatch and all three output sinks (UI timeline, session JSONL persistence, LLM conversation context).
  - Covers all 51 built-in tools, MCP tool results, and auto-heal compiler output.
  - No secret ever reaches the TUI display, log files, or the LLM API.
- **Constants & Infrastructure**:
  - Added `REDACTED_PLACEHOLDER` constant (`[REDACTED]`) in `src/constants.rs`.
  - Registered `redact` module in `src/sandbox/mod.rs`.
- **Verification & Testing**:
  - Added 20 unit tests in `src/sandbox/redact.rs` covering all 16 patterns, false positive guards, empty input, multi-secret strings, and env exact-match with minimum length threshold.
  - Added 7 integration tests in `tests/integration_secret_redaction.rs` simulating real tool output scenarios.
  - All tests passing with zero compiler or clippy warnings.

## [0.0.35] — 2026-08-20

### 🎨 Interactive Live Theme Switcher Modal (`/theme`)
- **Developer Color Palettes (`src/ui/theme.rs`)**:
  - Implemented full truecolor themes for top developer color schemes:
    - **Aura Dark (Default)** (Purple & Mint)
    - **Tokyo Night** (Midnight Blue & Lavender)
    - **Catppuccin Mocha** (Soothing Mauve & Rosewater)
    - **Nord Frost** (Arctic Slate & Polar Blue)
    - **Gruvbox Dark** (Retro Warm Amber & Forest Green)
    - **Dracula** (Vampire Purple & Emerald)
    - **Cyberpunk Matrix** (Neon Emerald, Electric Cyan & Pitch Black)
    - **Aura Soft Dark** & **ANSI 256 Fallback**
- **Visual Modal Selector (`src/ui/modal.rs`, `src/app.rs`)**:
  - Added `ModalState::ThemeSelect` rendered upon typing `/theme` or `/themes`.
  - Displays live colored swatch glyphs (`■`), titles, and descriptions with keyboard navigation (`↑`/`↓` to select, `Enter` to apply & save, `Esc` to cancel).
- **Live Switching & Automatic Persistence (`src/config.rs`, `src/app.rs`)**:
  - Implemented `Config::save(workspace_root)` which writes changes back to `.minicode/config.toml` or `~/.config/minicode/config.toml`.
  - Instantly recolors the running terminal interface on selection without restarting the application.
- **Verification & Testing**:
  - Added `tests/integration_theme_switcher.rs` testing palette uniqueness, theme detection, modal layout, and configuration file persistence.
  - Expanded test suite to **171 tests passing 100% green** with zero compiler or clippy warnings.

---

## [0.0.34] — 2026-08-20

### ⏪ Interactive Timeline Checkpoint Undo Engine (`/undo`)
- **Interactive Style 4 Timeline Graph Modal (`src/ui/modal.rs`, `src/app.rs`)**:
  - Replaced the blind single-turn `/undo` command with an interactive, git-style timeline graph modal with 0 emojis and clean typography.
  - Visual timeline nodes (`◉` active node, `○` past nodes, `│` connecting branch lines) display prompt text, human-readable relative timestamps (`2m ago`, `15s ago`), and modified file summaries.
  - Supports keyboard navigation with `↑`/`↓` to traverse earlier turns, `Enter` to revert to the selected checkpoint, and `Esc` to cancel.
- **Multi-Turn Cascading Rollback Engine (`src/session/undo.rs`, `src/session/backup.rs`)**:
  - Added `rollback_to_checkpoint(&workspace_root, target_turn_id)` to roll back multiple turns in sequence, restoring pre-turn file states and deleting newly created files added in rolled-back turns.
  - Automatically resets any intermediate git commits via `git reset --soft HEAD~N`.
- **Conversation State & Message Truncation (`src/agent/loop.rs`, `src/app.rs`)**:
  - Recorded turn start metadata (`user_prompt`, `message_index`) in `BackupManifest` at the start of each turn.
  - Automatically truncates agent LLM conversation history back to the selected checkpoint so the model's context window does not hallucinate from reverted turns.
- **Verification & Testing**:
  - Added `tests/integration_undo_checkpoint.rs` covering multi-turn file reversion, checkpoint discovery, and ratatui timeline graph rendering.
  - Expanded test suite to **168 tests passing 100% green** with zero compiler or clippy warnings.

---

## [0.0.33] — 2026-08-20

### 🏗️ Codebase Modularization, Domain Tool Registry & Unified Traversal Engine
- **Modular Domain Tool Registry (`src/tools/registry/`, `src/tools/mod.rs`)**:
  - Decomposed the monolithic 2,160 LOC `src/tools/mod.rs` registry into 7 domain submodules:
    - `fs_tools.rs` (`read_file`, `write_file`, `patch_file`)
    - `exec_tools.rs` (`exec_cmd`)
    - `search_tools.rs` (`grep_search`, `locate_symbol`, `semantic_search`, `ast_query`, `ast_extract_symbol`, `ast_diff`)
    - `git_tools.rs` (`git_status`, `git_diff`, `git_commit`, `git_log`, `git_conflicts`, `create_pr`)
    - `agent_tools.rs` (`delegate_task`, `create_task_dag`, `get_next_task`, `complete_task`, `critic_review`, `sequential_thinking`, `score_task_complexity`, `explore_hypotheses`, `evaluate_branch`, `select_best_branch`)
    - `context_tools.rs` (`remember_fact`, `update_fact`, `forget_fact`, `create_plan`, `read_plan`, `log_finding`, `update_progress`, `archive_plan`, `impact_analysis`, `repo_map`, `lsp_diagnostics`, `lsp_goto_definition`, `lsp_find_references`, `wiki_write`, `wiki_read`, `wiki_search`, `create_skill`, `list_skills`, `inspect_skill`, `check_architecture`, `prune_context`)
    - `web_tools.rs` (`fetch_or_browse`, `search_web`, `browser_navigate`, `browser_snapshot`)
  - Preserved 100% parameter signature parity across all 51 built-in agent tools with a concise facade router.
- **Unified Workspace File Traversal Engine (`src/context/walker.rs`)**:
  - Created canonical `WorkspaceWalker` abstracting `ignore::WalkBuilder` traversal, `.gitignore` awareness, hidden file filtering, depth limits, and standardized exclusions (`target`, `.git`, `node_modules`, `.venv`, `dist`, `build`).
  - Refactored `src/agent/complexity.rs`, `src/context/index.rs`, `src/context/governance.rs`, and `src/context/semantic.rs` to use the unified engine, eliminating over 100 lines of duplicated traversal code.
- **TUI Selection Decomposition (`src/ui/selection.rs`, `src/ui/view.rs`)**:
  - Extracted mouse drag text selection, coordinate tracking `(col, row)`, selection bounding boxes, multi-line visual highlight slicing, and clipboard copying into `TimelineSelection`.
- **Centralized Heuristic Constants (`src/constants.rs`)**:
  - Centralized task complexity risk keywords and cognitive memory decay parameters into `src/constants.rs`.
- **Verification & Testing**:
  - All 163 unit and integration tests passing 100% green with 0 clippy warnings and clean formatting.

---

## [0.0.32] — 2026-08-20

### 🎨 Fluid Thinking Animation, Clean Thought Stream & Configurable Cost Spend
- **Smooth 80ms Thinking Spinner (`src/ui/view.rs`, `src/app.rs`)**:
  - Replaced integer second animation ticks with high-resolution millisecond tracking (`working_millis`), achieving fluid 80ms frame cycling without terminal lag or stutter.
  - Formatted timer with subsecond precision: `⠋ Thinking (2.4s • esc to interrupt)`.
- **Clean Minimalist Thought Blocks (`src/ui/view.rs`)**:
  - Replaced vertical border box lines with clean two-space indented typography in Aura muted italics.
  - Removed all emojis across the interface for sleek, distraction-free CLI aesthetic.
  - Implemented exact thought completion header: `• Thought for {s}s` (e.g. `• Thought for 2.4s`, `• Thought for 21.0s`).
  - Fixed `0.0s` duration calculation bug by linking thought completion directly to elapsed turn execution time with fallback to `working_millis`.
- **Configurable Dollar Cost Spend Toggle (`src/config.rs`, `src/ui/status.rs`, `src/ui/configure.rs`)**:
  - Made session dollar spend an opt-in configuration toggle in `UiConfig` (`show_cost: bool`, defaults to `false`).
  - Keeps bottom status bar minimal and focused on context tokens (`4.2k / 128k`) by default.
  - Added option `[5]` in the interactive configuration wizard (`cargo run -- --configure`) to easily toggle cost display on or off.

---

## [0.0.31] — 2026-08-20

### 🎯 Task Complexity Scorer, Live Dollar Spend Telemetry & TUI Thinking Blocks
- **Hierarchical Task Complexity & Risk Scorer (`src/agent/complexity.rs`, `src/tools/mod.rs`)**:
  - Claude Task Master-inspired pre-execution complexity analyzer calculating difficulty score ($1$–$10$) and risk level (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`).
  - Estimates token context footprint, detects affected files, computes blast radius, and auto-generates multi-stage task decomposition plans.
  - Added new tool: `score_task_complexity` (expanding built-in suite to **51 agent tools total**).
- **Live Dollar Spend & Cost Telemetry in TUI Status Bar (`src/agent/pricing.rs`, `src/ui/status.rs`, `src/app.rs`)**:
  - Real-time pricing model across Anthropic Claude, OpenAI GPT, Google Gemini, DeepSeek, and local Ollama ($0.00/tok).
  - Displays live session dollar spend (e.g. `⚡ $0.0042 • 4.2k / 128k`) dynamically alongside token context utilization.
- **TUI "Thinking..." Spinner & Expandable Reasoning Thought Blocks (`src/ui/view.rs`, `src/app.rs`)**:
  - Added live animated `⠋ Thinking... ({}s • esc to interrupt)` spinner immediately upon prompt submission.
  - Formats thought processes and reasoning tokens inside a stylized Aura lavender/pink thought box (`💭 Thinking Process:`) with indented borders and italic styling before/alongside the final assistant response.
  - Automatically parses and separates `<thought>...</thought>` tags from assistant markdown streams.
- **Unit & Integration Tests**:
  - Added `tests/integration_complexity_scorer.rs` and `tests/integration_pricing_and_thoughts.rs`.
  - Expanded test suite to **153 tests passing 100% green** with zero clippy warnings.

---

## [0.0.30] — 2026-08-20

### 🏛️ Sentrux Architectural Governance & RTK Token Reduction Interceptor
- **Architectural Governance Sensor (`src/context/governance.rs`, `src/tools/mod.rs`)**:
  - Sentrux-inspired architectural sensor validating codebase modularity, acyclicity, and architectural boundaries.
  - Detects strongly connected components (SCCs) via Tarjan's algorithm to catch circular module dependency cycles.
  - Enforces strict architectural layer boundaries (e.g. preventing low-level tools/core from depending on UI/presentation).
  - Flags high-complexity "god files" (>1,000 LOC) and excessive fan-out modules (>10 imports).
  - Computes a deterministic Modularity Health Score ($0$–$100$).
  - Added new tool: `check_architecture` (milestone expansion to **50 agent tools total**).
- **RTK (Rust Token Killer) Output Interceptor (`src/tools/rtk_filter.rs`, `src/tools/exec.rs`)**:
  - High-performance command output filter cutting 60–90% of terminal log token waste.
  - Specialized parsing for `cargo test`, `pytest`, `npm test` / `jest`, `git log`, and verbose shell scripts.
  - Isolates failing test names, panics, assertion differences, and final test summaries while omitting repetitive passing spam.
- **Unit & Integration Tests**:
  - Added `tests/integration_governance.rs` and `tests/integration_rtk_filter.rs`.
  - Expanded test suite to **148 tests passing 100% green** with clean clippy.

---

## [0.0.29] — 2026-08-20

### 🌳 Context-Aware Semantic AST Code Diffing & Deterministic Replay Harness
- **Semantic AST Diffing Engine (`src/context/ast_diff.rs`, `src/tools/mod.rs`)**:
  - Tree-sitter AST structural delta analyzer for Rust, Python, TypeScript, and JavaScript.
  - Automatically identifies added, removed, and modified functions, classes, structs, and methods with signature vs. body change granularity.
  - Built-in breaking change detection heuristic highlighting public API mutations and removed symbols.
  - Added new tool: `ast_diff` (expanding built-in tools to **49 agent tools total**).
- **Deterministic Mock Provider & Replay Harness (`src/agent/mock_provider.rs`, `src/agent/replay.rs`)**:
  - In-memory mock LLM provider simulating streaming text deltas, tool calls, and API error injections for 100% offline regression testing.
  - Structured `.tape.jsonl` session recorder format capturing turn prompts, tool executions, and assistant completions.
  - Deterministic replay execution harness asserting exact tool sequences, arguments, and state transitions without live API costs.
- **Unit & Integration Tests**:
  - Added `tests/integration_ast_diff.rs` and `tests/integration_replay_harness.rs`.
  - Added unit tests for AST diffing and mock provider streaming.
  - Test suite expanded to **144 tests passing 100% green** with zero clippy warnings.

---

## [0.0.28] — 2026-08-20

### 📋 Live Mouse Drag Text Highlighting, Auto-Copy & Terminal Clipboard
- **Mouse Drag Selection & Auto-Copy (`src/ui/view.rs`, `src/app.rs`, `src/ui/clipboard.rs`)**:
  - Implemented OpenCode-style interactive text selection with real-time visual inverted highlighting (`Modifier::REVERSED`) across single-line, multi-line, and backwards mouse drags.
  - Automatically copies highlighted text to the system clipboard upon mouse release with zero external OS dependencies using pure-Rust terminal `OSC 52` escape sequences.
  - Displays instant inline confirmation toast showing the copied snippet preview (`✔ Copied to clipboard: "..."`).
  - Added dedicated `/copy` and `/copy all` slash commands in `src/ui/input.rs` for quickly copying the latest assistant response or the complete formatted conversation transcript.
  - Handled clean selection dismissal on <kbd>Esc</kbd>, single click, and prompt submission.
- **Rich Terminal Markdown Syntax Highlighter (`src/ui/markdown.rs`, `src/ui/view.rs`)**:
  - Panic-safe markdown tokenizer replacing unsafe byte slicing with UTF-8 character boundary scanning and `strip_prefix`/`strip_suffix`.
  - Vibrant Aura theme token highlights for file paths (mint green `#61ffca`), headings (Aura purple/pink), metrics/latencies/versions (orange `#ffca85`), and structured tables.
- **Status Bar Context & Layout Refinements (`src/ui/status.rs`, `src/app.rs`, `src/ui/input.rs`)**:
  - Right-aligned token context counter (`4.2k / 128k`) with dynamic threshold color coding (<60% green, 60-85% warning orange, >85% red).
  - Shaded elevated input dock (`theme.bg_input`) with rounded inner border and bottom margin spacer.
- **Unit & Integration Tests**:
  - Added `test_apply_selection_to_line`, `test_timeline_mouse_selection_and_copy`, `test_base64_encode`, `test_copy_to_clipboard`, `test_format_tokens`, and `test_markdown_edge_cases_no_panic`.
  - Test suite expanded to **141 tests passing 100% green** with zero clippy warnings.

---

## [0.0.27] — 2026-08-20

### 🖱️ Smooth Mouse Wheel & Multi-Key TUI Timeline Scrolling
- **TUI Scrolling Engine (`src/ui/view.rs`, `src/app.rs`, `src/ui/pty_drawer.rs`)**:
  - Enabled native mouse capture (`crossterm::EnableMouseCapture`) for mouse wheel and trackpad smooth scrolling across both the conversation timeline and the embedded terminal drawer.
  - Implemented dynamic auto-scroll state management with `Cell<u16>` and `Cell<bool>`, fixing the bug where scrolling up jumped straight to line 0 instead of stepping back from the bottom.
  - Added multi-key keyboard navigation: `PageUp`/`PageDown` (page scrolling), `Shift+↑/↓`, `Ctrl+↑/↓`, `Alt+↑/↓`, empty input dock `↑/↓`, and `Home`/`End` (jump to top/bottom).
  - Updated in-TUI Help modal (`/help`) with full scrolling shortcuts documentation.
- **Unit & Integration Tests**:
  - Added `test_timeline_scrolling_and_auto_scroll_resumption` in `src/ui/view.rs`.
  - Test suite expanded to **132 tests passing 100% green** with zero clippy warnings.

---

## [0.0.26] — 2026-08-18

### 🌲 Dynamic Multi-Branch Hypothesis Search & Speculative Rollout
- **Hypothesis Engine (`src/agent/hypothesis.rs`)**:
  - Implemented parallel speculative branch exploration in isolated Git worktrees for evaluating competing implementation strategies.
  - Objective branch fitness evaluation using automated compiler diagnostics (`FastCompilerChecker`), error/warning counts, and normalized fitness scores ($0.0$–$1.0$).
  - Automatic winning branch selection and instant clean-up of temporary alternative speculative worktrees.
  - Added 3 new tools: `explore_hypotheses`, `evaluate_branch`, `select_best_branch` (expanding built-in tools to **48 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_hypothesis_tree.rs` validating multi-branch lifecycle, fitness scoring, winner selection, and tool dispatch.
  - Test suite expanded to **131 tests passing 100% green** with zero clippy warnings.

---

## [0.0.25] — 2026-08-18

### 🗜️ Multi-Turn Context Observation Deduplication
- **Observation Deduplicator (`src/context/dedup.rs`)**:
  - Implemented automatic content fingerprinting and deduplication for repetitive tool observations across long multi-turn sessions.
  - Collapses duplicate file reads into compact reference pointers preserving line counts and turn anchors.
  - Deduplicates repeating compiler diagnostics and linter outputs into summarized status indicators.
  - Automatically integrated into the agent execution loop's `prune_context()` pipeline.
  - Added new tool: `prune_context` (expanding built-in tools to **45 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_context_dedup.rs` validating observation collapsing, character savings, and tool dispatch.
  - Test suite expanded to **128 tests passing 100% green** with zero clippy warnings.

---

## [0.0.24] — 2026-08-18

### 🌳 Tree-sitter AST Pattern Matching & Symbol Extraction
- **AST Transformer Engine (`src/context/ast_transform.rs`)**:
  - Implemented multi-language Tree-sitter AST structural query engine supporting Rust, Python, JavaScript, and TypeScript.
  - Granular node kind and symbol name filtering with visibility modifier detection (`pub`, `export`).
  - Exact symbol extraction with complete syntax body and line coordinates for precise AI context injection.
  - Added 2 new tools: `ast_query` and `ast_extract_symbol` (expanding built-in tools to **44 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_ast_transform.rs` validating AST node queries, filtering, and symbol extraction across Rust, Python, and TypeScript.
  - Test suite expanded to **125 tests passing 100% green** with zero clippy warnings.

---

## [0.0.23] — 2026-08-18

### 🔍 Sub-millisecond Local Semantic Code Search
- **Semantic Code Vector Index (`src/context/semantic.rs`)**:
  - Implemented 100% offline, pure-Rust localized semantic embedding index utilizing 128-dimensional character n-gram and subword hashing projections (FastText/Model2Vec style).
  - Sliding window source chunking (~25 lines) with line-span coordinates and cosine similarity ranking.
  - Disk-backed index cache in `.minicode/cache/semantic_index.json` with mtime-based incremental updates.
  - Added new tool: `semantic_search` (expanding built-in tools to **42 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_semantic_search.rs` validating source chunking, vector projections, cosine similarity retrieval, and tool dispatch.
  - Test suite expanded to **122 tests passing 100% green** with zero clippy warnings.

---

## [0.0.22] — 2026-08-18

### 🛠️ Dynamic Skill Creation & Hot-Reloading Engine
- **Dynamic Skill Forge (`src/context/skill_forge.rs`)**:
  - Allows autonomous agents and developers to forge, validate, and hot-load project-specific skill packages into `.minicode/skills/<name>/SKILL.md`.
  - Automatic YAML frontmatter serialization with metadata (`name`, `description`, `version`, `author`, `allowed_tools`).
  - Hot-reloading integration with `SkillDiscoverer` for zero-restart skill activation.
  - Added 3 new tools: `create_skill`, `list_skills`, and `inspect_skill` (expanding built-in tools to **41 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_skill_forge.rs` validating skill creation, metadata parsing, directory inspection, and tool execution.
  - Test suite expanded to **118 tests passing 100% green** with zero clippy warnings.

---

## [0.0.21] — 2026-08-16

### ⚡ Interactive Embedded PTY Terminal Drawer
- **Embedded PTY Terminal Drawer (`src/ui/pty_drawer.rs`)**:
  - Implemented interactive bottom drawer overlay (bottom 40% viewport) with Aura Mint Green borders.
  - Bounded 1000-line output ring buffer with auto-scroll and color-coded status styling.
  - Direct shell command execution without leaving the Ratatui TUI session.
  - Fast keyboard shortcuts: `Ctrl+T` to toggle, `Esc` to close, `Enter` to run.
  - Added `/terminal` slash command for terminal drawer management.
- **Integration Test Suite**:
  - Added `tests/integration_pty_drawer.rs` validating drawer state toggle, line wrapping, ring buffer limits, and test backend rendering.
  - Test suite expanded to **115 tests passing 100% green** with zero clippy warnings.

---

## [0.0.20] — 2026-08-16

### 🌐 ARIA Web Browser & Local UI Dev Inspector
- **ARIA Accessibility Tree Generator (`src/tools/browser.rs`)**:
  - Implemented pure-Rust DOM & accessibility tree extractor for live web pages and local dev servers (`http://localhost:3000`, `http://localhost:8080`).
  - Converts interactive controls (`<button>`, `<a>`, `<input>`, `<select>`, `<textarea>`) into numbered element references (`@e1`, `@e2`, ...).
  - Extracts form actions, input names/types, and links for testing web applications.
  - Added tools: `browser_navigate` and `browser_snapshot` (expanding built-in tools to **38 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_browser.rs` validating HTML accessibility tree generation, element reference resolution, and tool dispatch.
  - Test suite expanded to **112 tests passing 100% green** with zero clippy warnings.

---

## [0.0.19] — 2026-08-16

### 📚 Cognitive Memory Decay & Repository Knowledge Wiki
- **Exponential Cognitive Memory Decay (`src/context/decay.rs`)**:
  - Implemented Ebbinghaus-inspired temporal memory retention modeling ($R(t) = \exp(-\ln(2) \cdot t / (H \cdot S))$).
  - Scope partitioning: `Permanent` (zero decay for repo rules), `Milestone` (7-day half-life for active goals), and `Transient` (60-minute half-life for episodic debug traces).
  - Automatic reinforcement boosts stability factor upon re-access.
- **Compounding Knowledge Wiki Engine (`src/context/wiki.rs`)**:
  - Filesystem-backed Markdown knowledge base in `.minicode/wiki/<topic>.md`.
  - Automatic `.minicode/wiki/index.md` cataloging and topic frontmatter serialization.
  - Added tools: `wiki_write`, `wiki_read`, and `wiki_search` (expanding built-in tools to **36 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_wiki_and_decay.rs` validating biological memory retention decay, pruning, and full wiki CRUD tool lifecycle.
  - Test suite expanded to **109 tests passing 100% green** with zero clippy warnings.

---

## [0.0.18] — 2026-08-16

### 🔀 Sequential Thinking & Graph of Thoughts (GoT) Reasoning
- **Graph of Thoughts Reasoning Engine (`src/agent/sequential_thinking.rs`)**:
  - Implemented dynamic hypothesis branching, revision tracking, and confidence scoring (`score: 0.0`–`1.0`).
  - Tracks non-linear thinking trajectories in a directed graph using `petgraph`.
  - Added tool: `sequential_thinking` (expanding built-in tools to **33 agent tools total**).
  - Generates synthesized outline summaries upon convergence for complex debugging and architectural planning turns.
- **Integration Test Suite**:
  - Added `tests/integration_sequential_thinking.rs` validating thought node progression, hypothesis branching, and tool dispatch.
  - Test suite expanded to **104 tests passing 100% green** with zero clippy warnings.

---

## [0.0.17] — 2026-08-16

### 🧠 Topological Task DAG & Actor-Critic Verification Engine
- **Petgraph-Powered Task DAG Engine (`src/agent/task_dag.rs`)**:
  - Implemented dependency-managed Directed Acyclic Graph (DAG) for multi-step feature execution.
  - Cycle detection and topological execution ordering via `petgraph::algo::toposort`.
  - Heuristic complexity scoring engine (1–10 scale) based on scope, affected file depth, and risk keywords.
  - Added tools: `create_task_dag`, `get_next_task`, and `complete_task`.
- **Actor-Critic Quality Gate (`src/agent/critic.rs`)**:
  - Automated dual-agent verification pass running compiler diagnostics, linter checks, and git modification analysis.
  - Added tool: `critic_review` (expanding built-in tools to **32 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_task_dag.rs` validating dependency graphs, topological order resolution, unblocked task queries, and critic reviews.
  - Test suite expanded to **101 tests passing 100% green** with zero clippy warnings.

---

## [0.0.16] — 2026-08-16

### 📦 Multi-Platform Release Matrix & Single-Command Distribution
- **Automated Multi-Platform Release Matrix (`.github/workflows/release.yml`)**:
  - Implemented GitHub Actions release workflow triggering on version tags (`v*`).
  - Cross-compilation and automated asset packaging for 5 tier-1 platforms:
    - `x86_64-unknown-linux-gnu` (Linux x86_64 tar.gz + sha256)
    - `aarch64-unknown-linux-gnu` (Linux ARM64 tar.gz + sha256)
    - `aarch64-apple-darwin` (macOS Apple Silicon tar.gz + sha256)
    - `x86_64-apple-darwin` (macOS Intel tar.gz + sha256)
    - `x86_64-pc-windows-msvc` (Windows x86_64 zip + sha256)
- **One-Line Curl Installer (`install.sh`)**:
  - Auto-detects operating system (`uname -s`) and CPU architecture (`uname -m`).
  - Downloads the matching release archive directly from GitHub Releases and installs to `~/.local/bin/minicode`.
  - Gracefully falls back to `cargo install` if GitHub API rate limits occur.
- **Documentation & PRD Synchronization**:
  - Updated all feature tracker checklists, tool inventories (28 agent tools), and architecture specs in `onpkg_docs/`.

---

## [0.0.15] — 2026-08-16

### 🔍 Native Web Search Engine & Anti-Scrape Cache
- **Native Web Search Engine (`src/tools/web_search.rs`)**:
  - Implemented multi-provider web search tool `search_web` (expanding built-in tools to **28 agent tools total**).
  - **Zero-API-key DuckDuckGo Scraper**: High-resilience HTML endpoint scraping with HTML parser, title & snippet extraction, and clean Markdown links.
  - **In-Memory TTL Search Cache**: Thread-safe 15-minute query caching to avoid rate-limiting during multi-turn planning sessions.
  - **Optional API Fallbacks**: Seamlessly integrates with Tavily (`TAVILY_API_KEY`) and Brave Search (`BRAVE_API_KEY`) when environment variables are set.
- **Integration Test Suite**:
  - Added `tests/integration_web_search.rs` verifying markdown search result compilation, link formatting, and empty query rejection.
  - Full test suite expanded to **96 tests passing 100% green** with zero clippy warnings.

---

## [0.0.14] — 2026-08-16

### 🛡️ Interactive TUI Diff Inspector & Permission Menu Modal
- **Syntax-Highlighted Unified Diff Viewer (`src/ui/diff_viewer.rs`)**:
  - Implemented terminal diff engine powered by `similar`, rendering clean colored unified diff lines matching Dalton Menezes' Aura Theme (`+` Aura Mint Green additions, `-` Aura Coral Red deletions, muted context).
  - Truncates oversized diff hunks gracefully with line counts.
- **Interactive 4-Option Permission Selection Modal (`src/ui/approval.rs`, `src/ui/modal.rs`, `src/app.rs`)**:
  - Replaced legacy single-key prompts with an interactive Aura modal menu displaying target action details and proposed changes.
  - Interactive navigable options (`↑` / `↓` / `j` / `k` or direct numbers `1`–`4`, `Enter` to confirm):
    - `[1] Accept & Apply (Execute action)`
    - `[2] Reject (Decline this action)`
    - `[3] Allow for this Session (Auto-approve subsequent turns)`
    - `[4] Type Feedback / Custom Instructions (Guide agent)`
  - Direct steering support: When option 4 is selected, an inline feedback input dock opens, sending custom instructions back to the agent loop.
- **Integration Test Suite**:
  - Added `tests/integration_diff_modal.rs` testing diff formatting, modal navigation, and custom feedback typing.
  - Test suite expanded to **92 tests passing 100% green** with zero clippy warnings.

---

## [0.0.13] — 2026-08-16

### 🚀 Language Server Protocol (LSP) Engine & 2-Tier Compiler Diagnostics
- **Pure-Rust Stdio JSON-RPC 2.0 Client (`src/lsp/protocol.rs`, `src/lsp/client.rs`)**:
  - Implemented async stdio JSON-RPC 2.0 framing with strict `Content-Length:` header reading and serialization.
  - Auto-discovery for language servers: `rust-analyzer` (Rust), `pyright` (Python), `typescript-language-server` (TypeScript/JavaScript), and `gopls` (Go).
  - Handles initialization handshakes, non-blocking requests with timeouts, and clean child process lifecycle management (`SIGTERM` / `SIGKILL` on drop).
- **2-Tier Hybrid Compiler Diagnostics Engine (`src/lsp/diagnostics.rs`, `src/lsp/mod.rs`)**:
  - **Tier 1 (Instant Fast-Path)**: Direct compiler CLI checks (`cargo check --message-format=json`, `tsc --noEmit`, `ruff check`) running in < 200ms with zero RAM overhead.
  - **Tier 2 (Deep Semantic LSP)**: Asynchronous LSP client providing live diagnostics, `lsp_goto_definition`, and `lsp_find_references`.
  - Added 3 new agent tools: `lsp_diagnostics`, `lsp_goto_definition`, and `lsp_find_references` (expanding built-in tools to **27 agent tools total**).
- **Autonomous Compiler Self-Healing Loop (`src/agent/loop.rs`, `src/config.rs`)**:
  - Automatically queries workspace compiler diagnostics after file modifications.
  - If compiler errors are detected, the agent receives an immediate structured feedback prompt with line, column, and rustc/tsc error spans, automatically fixing syntax or type errors before completing the turn.
  - Configurable via `[agent] auto_heal = true` in `config.toml`.
- **Integration Test Suite**:
  - Added `tests/integration_lsp_diagnostics.rs` validating protocol framing, diagnostic formatting, and tool dispatch.
  - Total test count expanded to **86 tests passing 100% green** with zero clippy warnings.

---

## [0.0.12] — 2026-08-16

### 🚀 Autonomous Git Engine & Worktree Orchestration
- **Autonomous Git Service Engine (`src/git/service.rs`, `src/git/commit.rs`, `src/git/diff_filter.rs`, `src/git/worktree.rs`)**:
  - Implemented hardened Git engine using isolated async subprocesses (`tokio::process::Command`) with mandatory flags: `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `LC_ALL=C`, and `--no-pager`.
  - Added token-budgeted `DiffFilter` that automatically collapses multi-thousand line lockfiles (`Cargo.lock`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, etc.) and enforces strict byte budgets (`GIT_DIFF_MAX_BYTES`).
  - Added 6 autonomous agent git tools: `git_status`, `git_diff`, `git_commit`, `git_log`, `git_conflicts`, and `create_pr` (24 built-in agent tools total).
  - Implemented `WorktreeManager` supporting isolated parallel subagent branches (`subagent/<id>`) and zero-conflict branch merging.

### 🤖 Autonomous Post-Turn Auto-Commit & Reversible Rollbacks
- **Autonomous Auto-Commit Loop (`src/agent/loop.rs`, `src/config.rs`)**:
  - Automatically commits modified files after successful agent turns using generated Conventional Commits messages (`feat: ...`, `fix: ...`, `docs: ...`).
  - Configurable in `config.toml` (`[git] auto_commit = true, dirty_commit = false, ai_commit_messages = true`).
  - Synchronized `/undo` rollback to execute `git reset --soft HEAD~1` alongside filesystem checkpoint restoration.

### 🧪 Comprehensive Integration Test Suite & CI/CD Pipeline
- **GitHub Actions CI (`.github/workflows/ci.yml`)**:
  - Full CI pipeline validating code formatting (`cargo fmt --check`), strict clippy lints (`cargo clippy -- -D warnings`), and cross-platform tests across Linux (`Ubuntu`) and macOS (`macOS`).
- **Two-Tier Integration Test Framework (`tests/`)**:
  - Created `src/lib.rs` and `tests/common/mock_provider.rs` for deterministic multi-turn simulation without network overhead.
  - Added `tests/integration_agent_loop.rs` verifying multi-turn read-and-patch turns and user cancellation.
  - Added `tests/integration_git_tools.rs` verifying autonomous git tools, auto-commits, and undo rollbacks.
  - Added `tests/integration_subagents.rs` verifying parallel worktrees and branch merging.
  - **80/80 tests passing 100% green**.

### 🖥️ Session Controls & Codebase Observability
- **Extended Slash Commands (`src/ui/input.rs`, `src/app.rs`)**:
  - `/retry`: Re-submits the last prompt to the agent loop.
  - `/save [path]`: Exports the current session timeline to formatted Markdown.
  - `/load [id]`: Hydrates timeline from past session stores.
  - `/map`: Renders the AST PageRank repository map directly in the timeline.
  - `/compact`: Triggers context token compaction.
  - `/tokens`: Displays detailed token breakdown and context window metrics.

---

## [0.0.11] — 2026-08-16

### 🚀 Critical Fixes & Resilience Upgrades
- **Gemini Multi-Tool Turn Merging & ID Support (`src/agent/provider.rs`)**:
  - Merged adjacent `Role::Tool` messages into a single `role: "user"` message with multiple `functionResponse` parts to adhere strictly to the Gemini multi-turn API format and prevent HTTP 400 Bad Request errors.
  - Attached call `id` to both `functionCall` and `functionResponse` for seamless Gemini 2.0+ tool call matching.
- **CodeGraph Dangling Node Mass Redistribution & L1 Normalization (`src/context/graph.rs`)**:
  - Resolved graph probability leakage by redistributing dangling node mass across all nodes during PageRank power iteration, followed by exact L1 normalization ($\sum P_i = 1.0$).
  - Upgraded symbol-to-file lookup to support multiple declarations of identical identifiers across distinct files with keyword noise filtering.
- **Cooperative Agent Cancellation (`src/agent/loop.rs`, `src/app.rs`, `Cargo.toml`)**:
  - Integrated `tokio_util::sync::CancellationToken` into `AgentLoop::execute_turn` to enable immediate and clean cancellation on `Esc` / `Ctrl+C` across LLM streaming, token processing, and tool dispatching.
  - Fixed token usage inflation by tracking `last_prompt_tokens + cumulative_completion_tokens`.
- **Automatic Backup Manifest Persistence (`src/session/backup.rs`)**:
  - Safety checkpoint creation now automatically creates or updates the turn's `manifest.json` on disk using validated absolute workspace paths.

### 🛡️ Security & Sandbox Hardening
- **SSRF Network Protection (`src/tools/web.rs`, `src/constants.rs`, `src/error.rs`)**:
  - Added strict SSRF validation to `fetch_or_browse` blocking localhost, IPv4 loopback (`127.0.0.0/8`), IPv6 loopback (`::1`), link-local (`169.254.0.0/16`), private subnets (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `100.64.0.0/10`), and cloud metadata endpoints (`169.254.169.254`).
- **Landlock Kernel Compatibility & Graceful Degradation (`src/sandbox/landlock.rs`)**:
  - Added robust detection for unsupported host kernels (`ENOSYS`, `EOPNOTSUPP`), allowing WSL2, Docker containers, and older Linux kernels to degrade gracefully without crashing.
- **Process Group Termination with SIGKILL Escalation (`src/tools/exec.rs`, `Cargo.toml`)**:
  - Subprocess timeouts now send `SIGTERM` followed by a grace period before escalating to `SIGKILL` directly via `libc::kill`.

### ⚡ Context Engine, UI & Configuration
- **Expanded Tree-sitter Queries (`src/context/repomap.rs`)**:
  - Extended JavaScript and TypeScript queries to extract class methods, arrow functions, `enum_declaration`, and interface properties.
  - Hardened AST cache invalidation using a `(modified_time, file_size)` tuple.
- **Gemini Header Authentication & Stream Error Extraction (`src/agent/provider.rs`, `src/agent/models.rs`)**:
  - Switched Gemini API authentication to standard `x-goog-api-key` header instead of URL query parameters.
  - Extracted prompt-level policy blocks (`blockReason`), API stream errors, and candidate `finishReason` stops (`SAFETY`, `RECITATION`, `BLOCKLIST`).
- **Ollama TOML Configuration & Status Display (`src/config.rs`, `src/ui/status.rs`, `src/app.rs`)**:
  - Added `pub ollama: Option<OllamaConfig>` to `RawProviderConfig` and merged in `merge_raw`.
  - Updated Aura TUI status bar to display the active provider along with the model name (e.g., `gemini:gemini-2.5-pro`).

---

## [0.0.9] — 2026-08-16

### 🚀 Critical Bug Fixes & Reliability Hardening
- **Google Gemini Function Calling Name Alignment (`src/agent/types.rs`, `src/agent/loop.rs`, `src/agent/provider.rs`)**:
  - Attached real `tool_name` to `Message::tool_result` constructor so Gemini receives declared tool names (`read_file`, `write_file`) in `functionResponse.name` rather than synthetic UUIDs, preventing API 400 Bad Request errors.
- **MCP Client Multi-Server Tool Discovery & Stdio Streaming (`src/mcp/client.rs`, `src/constants.rs`)**:
  - Implemented `discover_server_tools` to dynamically query connected stdio and HTTP/SSE MCP servers via `tools/list` on initialization.
  - Replaced blocking `wait_with_output()` with asynchronous `BufReader::lines()` streaming reader to support persistent stdio MCP daemons without timing out.
  - Added robust `extract_tool_output` with `isError: true` payload extraction.
- **Child Process Pipe Drain & Process Group Termination (`src/tools/exec.rs`)**:
  - Concurrently drains subprocess pipe buffer overflow to `tokio::io::sink()` to prevent child processes from blocking on full OS pipe buffers when output exceeds 512 KB.
  - Enforces process group isolation with `process_group(0)` on Unix and terminates full process trees on timeout.
- **MCP Server Full Tool Parity (`src/mcp/server.rs`)**:
  - Delegated MCP server tool execution directly to `ToolRegistry::dispatch` to expose all 17 coding tools with backup snapshots, sandbox enforcement, and standard `isError` response payloads.
- **Backup & Undo Workspace Path Confinement (`src/session/backup.rs`, `src/session/undo.rs`)**:
  - Enforced `validate_path_in_workspace` on safety checkpoint creation and undo rollbacks to eliminate path traversal vulnerabilities.

### 🧠 Context Engine & Search Enhancements
- **Context Pruning Conversation Turn Invariant (`src/agent/loop.rs`)**:
  - Guaranteed that pruning never leaves an orphaned `Role::Tool` message at the beginning of LLM conversation history.
- **Observation Masking Dynamic Budget (`src/context/compressor.rs`)**:
  - Derived head and tail lines dynamically from `max_lines` to prevent zero or negative truncation slices.
- **BM25 Prefix Range Query Deduplication & IDF Calculation (`src/context/index.rs`)**:
  - Deduplicated postings per query token during BTreeMap prefix range scans and computed correct document frequency for prefix matches.
- **Deterministic Parallel Tool Call Ordering (`src/agent/provider.rs`)**:
  - Switched tool call streaming accumulator to `BTreeMap<usize, ...>` to guarantee deterministic execution order across parallel tool dispatches.
- **Landlock Sandbox Permitted Paths Expansion (`src/sandbox/landlock.rs`)**:
  - Added `/dev`, `/proc`, `/opt`, `/usr/local`, and user toolchain directories (`~/.cargo`, `~/.rustup`, `~/.nvm`, `~/.local`) to Landlock permitted read paths.
- **Search File Pattern Matching for Nested Paths (`src/tools/search.rs`)**:
  - Supported filename and relative path glob matching for patterns like `*.rs` and `client.rs` across nested directories.

### ⚡ Interactive TUI State Sync
- **Runtime Configuration Synchronization (`src/app.rs`, `src/agent/loop.rs`)**:
  - Implemented `AgentCommand::UpdateConfig` channel to immediately sync provider, model, and API key updates from the in-TUI modal dialog to the active background agent loop.
- **Doc Comment Attribute Parsing (`src/context/repomap.rs`)**:
  - Skipped Rust attributes (`#[...]`, `#![...]`) and Python decorators (`@...`) when extracting doc comments in Tree-sitter AST repo mapping.
- **Working Memory Findings Prompt Injection (`src/context/working_memory.rs`)**:
  - Linked discoveries and key findings from `findings.md` into the active `<working_memory>` system prompt block.

---

## [0.0.8] — 2026-08-15

### 🧠 Context Window Management & Memory Protection
- **Sliding Window Conversation Context Pruner (`src/agent/loop.rs`, `src/constants.rs`)**:
  - Implemented `prune_context()` in `AgentLoop` which actively monitors token budget using `ContextCompressor`.
  - Automatically compacts older tool observations and prunes excess oldest conversation messages when approaching `CONTEXT_WINDOW_PRUNE_THRESHOLD` (100,000 tokens) while guaranteeing preservation of recent turns (`CONTEXT_MIN_PRESERVED_MESSAGES`).
  - Added unit tests `test_prune_context_preserves_minimum_messages` and `test_prune_context_compacts_and_prunes_large_history`.

### 🛡️ Error Visibility & Resilience
- **Model Cache I/O Warning Logs (`src/agent/models.rs`)**:
  - Replaced silent `.ok()` error swallowing with structured `tracing::warn!` logging for model cache directory creation, file write, and corrupted JSON read recovery.
- **Temp File Atomic Write Cleanup (`src/tools/fs.rs`, `src/context/memory.rs`)**:
  - Logged warnings on temporary file removal failures during atomic write rollback instead of discarding errors silently.

### 🧹 Dead Code & Interface Polish
- **Removed Blanket `#![allow(dead_code)]` (`14 files`)**:
  - Eliminated file-level blanket dead code suppression across `agent/`, `context/`, `session/`, `sandbox/`, and `ui/` modules.
  - Replaced with targeted, item-level `#[allow(dead_code)]` attributes on specific public API structs and methods.
- **Provider & Directory Constants Centralization (`src/constants.rs`, `src/agent/provider.rs`, `src/agent/models.rs`, `src/tools/exec.rs`, `src/tools/mod.rs`)**:
  - Centralized `GEMINI_BASE_URL`, `OPENROUTER_BASE_URL`, `PROJECT_REPO_URL`, `PROVIDER_STREAM_TIMEOUT_SECS`, `PROVIDER_REQUEST_TIMEOUT_SECS`, `MODELS_CACHE_FILE`, `SIGNAL_KILLED_EXIT_CODE`.
  - Replaced hardcoded provider URLs, timeouts, and magic numbers across the agent and tool subsystems.
- **Updated Tool Registry Documentation (`src/tools/mod.rs`)**:
  - Corrected stale tool count doc comment in `ToolRegistry::get_tool_schemas()`.

---

## [0.0.7] — 2026-08-15

### 🔒 Reliability & Security Hardening
- **Stream Retry State Reset (`src/agent/loop.rs`)**:
  - Automatically clears `iteration_text`, `pending_tool_calls`, and truncates `turn_response` upon stream errors before retrying, preventing output duplication and malformed tool calls.
- **Strict Landlock Error Propagation (`src/tools/exec.rs`)**:
  - Propagated `apply_landlock_sandbox` errors in `pre_exec` hook as `PermissionDenied` so command execution strictly aborts if kernel-level sandboxing fails.
- **Landlock Network Fallback Warning (`src/sandbox/landlock.rs`)**:
  - Emits visible `tracing::warn!` when running on Linux kernels lacking Landlock ABI V4 where TCP network restriction cannot be enforced.
- **Web Response Payload Limit (`src/tools/web.rs`, `src/constants.rs`)**:
  - Enforced `MAX_WEB_RESPONSE_BYTES` (10 MB) via `Content-Length` and byte buffering to prevent out-of-memory denial-of-service on massive web downloads.
- **Plan Archiving Integrity (`src/context/working_memory.rs`)**:
  - Propagated I/O errors during archive reading to guarantee active plans and progress files are never deleted if reading fails.

### ⚡ Performance & Caching Optimizations
- **Single-Pass AST File Content Caching (`src/context/graph.rs`)**:
  - Cached file contents in memory during Tree-sitter AST extraction to eliminate duplicate disk reads during dependency edge building.
- **$O(1)$ Test Coverage Deduplication (`src/context/graph.rs`)**:
  - Replaced $O(N^2)$ `Vec` lookups with `HashSet` in blast radius test coverage correlation.
- **HTTP `Retry-After` Header Parsing (`src/agent/provider.rs`)**:
  - Extracted and honored provider-specified `Retry-After` seconds on HTTP 429 rate limit responses.
- **TUI Ticker Drift Protection (`src/app.rs`)**:
  - Configured `MissedTickBehavior::Skip` on TUI 50ms interval ticker to avoid UI frame bursts.

### 🧹 Cleanliness, Centralization & Polish
- **Environment API Key Trimming (`src/config.rs`)**:
  - Trimmed surrounding whitespace and newlines from all `.env` and environment variable API keys.
- **Undo Directory Removal (`src/session/undo.rs`)**:
  - Added recursive `remove_dir_all` cleanup for directories created during rolled-back turns.
- **Constants Centralization (`src/constants.rs`, `src/agent/models.rs`, `src/context/index.rs`, `src/context/compressor.rs`, `src/mcp/server.rs`)**:
  - Consolidated model provider URLs, fetch timeouts, `BM25_PREFIX_WEIGHT`, `COMPRESSOR_MASK_LINES`, `DEFAULT_LOCATE_SYMBOL_LIMIT`, and `SUPPORTED_LANG_EXTENSIONS`.
- **Grep Skipped File Tracing (`src/tools/search.rs`)**:
  - Added `tracing::debug!` logging when unreadable files are skipped during search.

---

## [0.0.6] — 2026-08-15

### ⚡ Performance & Indexing Optimization
- **$O(V \times I)$ Graph Edge Construction (`src/context/graph.rs`)**:
  - Replaced $O(V \times S)$ quadratic full-corpus substring searches with a single-pass `HashSet` identifier lookup, avoiding massive CPU stalls on large repositories.
- **Robertson-Spärck Jones BM25 Scoring (`src/context/index.rs`)**:
  - Implemented the standard BM25 formula incorporating document frequency (IDF), document length normalization ($k_1 = 1.2, b = 0.75$), type definition boosts (+3.0 for struct/class/interface/trait/enum, +2.0 for functions), and test/mock file down-ranking (*0.5).
- **$O(\log N + K)$ Prefix Range Lookups (`src/context/index.rs`)**:
  - Replaced linear `HashMap` scan with `BTreeMap` range queries (`.range(prefix..prefix_end)`).
  - Preserved single-character domain variables/identifiers (e.g. `x`, `y`, `e`).
- **Zero-Allocation Observation Masking (`src/context/compressor.rs`)**:
  - Optimized `mask_observation` to stream head and tail lines with iterators (`.take()`, `.skip()`) without collecting the full output into a temporary `Vec<&str>`.

### 🛠️ Tool Registry & Protocol Alignment
- **Tool Suite Synchronization (`src/tools/mod.rs`, `src/mcp/server.rs`)**:
  - Synchronized `repo_map` into `ToolRegistry::get_tool_schemas()` and `dispatch_tool` (totaling 17 built-in LLM tools and 8 MCP protocol endpoints).
  - Added `parse_u64_param` helper across built-in and MCP tool dispatchers to robustly parse both JSON integers and stringified numbers.
- **JSON-RPC Protocol Hardening (`src/mcp/server.rs`)**:
  - Added strict object shape validation for `params` and `arguments` in `tools/call`.
- **Search ReDoS Guard (`src/tools/search.rs`)**:
  - Added `MAX_REGEX_QUERY_LEN` protection (1024 characters) against oversized regex patterns.

### 📦 Centralized Constants & Architecture Cleanliness
- **Constants Centralization (`src/constants.rs`, `src/sandbox/env.rs`, `src/session/`, `src/context/skills.rs`)**:
  - Consolidated Blast Radius risk thresholds, BM25 tuning parameters, directory names (`SESSIONS_DIR_NAME`, `BACKUPS_DIR_NAME`, `SKILL_MD_FILE`, `SKILLS_DIR_NAME`, `MCP_TOOL_PREFIX`), and sandbox environment sanitization arrays (`WHITELIST_ENV_VARS`, `SECRET_PATTERNS`, `BLOCKED_PREFIXES`).
  - Added `#[must_use]` on `build_system_prompt` (`src/agent/prompt.rs`).

---

## [0.0.5] — 2026-08-15

### 🧠 AST Code Intelligence & Symbol Extraction
- **Rich AST Signature & Doc Extraction (`src/context/repomap.rs`)**:
  - Upgraded `SymbolDef` to extract clean single-line signatures (e.g. `pub fn compute_sum(...) -> i32`, `class UserService:`, `export interface UserProfile`), line spans (`start_line`, `end_line`), and preceding documentation comments (`///`, `//!`, `#`).
  - Added Tree-sitter query extraction for Rust, Python, JavaScript, and TypeScript encompassing functions, structs, classes, interfaces, traits, enums, type aliases, and module imports.

### 🌐 Code Knowledge Graph & Blast Radius Analysis
- **Blast Radius & Impact Analysis (`src/context/graph.rs`)**:
  - Implemented `get_blast_radius` evaluating downstream ripple effects of modifying symbols or files across the codebase.
  - Multi-hop transitive dependency BFS traversal ($k=3$).
  - Automated test suite correlation (`tests/`, `*_test.rs`, `test_*`) identifying test coverage.
  - **Tarjan SCC Cycle Detection**: Utilized `petgraph::algo::tarjan_scc` to identify mutual recursive dependency cycles.
  - Formatted architectural risk ratings (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`) with Markdown report generation.

### ⚡ Sub-Millisecond Inverted Symbol Index
- **Inverted Symbol Index with BM25 Scoring (`src/context/index.rs`)**:
  - Subword tokenization supporting `camelCase`, `snake_case`, `SCREAMING_SNAKE_CASE`, and kebab notation.
  - Definition boosts (+30% for structs, classes, interfaces, traits) and automated penalty down-ranking for test/mock files.
  - Fast `locate_symbol` and `search_symbols` routines.

### 🛠️ New Agent & MCP Protocol Tools
- **Architectural Tools Integration (`src/tools/mod.rs`, `src/mcp/server.rs`)**:
  - Registered `impact_analysis` and `locate_symbol` in the built-in LLM tool registry.
  - Exposed `impact_analysis` and `locate_symbol` over the Model Context Protocol (MCP) `tools/list` and `tools/call`.

---

## [0.0.4] — 2026-08-15

### 🛡️ Security & Sandboxing
- **Landlock Async Isolation (`src/sandbox/landlock.rs`, `src/tools/exec.rs`)**:
  - Moved Landlock kernel rule enforcement (`ruleset.restrict_self()`) into Linux `std_cmd.pre_exec(...)` hooks after `fork()` so Tokio async worker threads are never restricted.
  - Added explicit read-write access to `/tmp` for build tools and test suites while restricting `/usr`, `/lib`, `/etc`, and `/bin` to read-only.
- **Environment Variable Sanitization (`src/sandbox/env.rs`)**:
  - Expanded secret stripping with 19 vendor blocked prefixes and sensitive patterns (`DATABASE_URL`, `SENTRY_DSN`, `KUBECONFIG`, `DOCKER_HOST`, `SSH_AUTH_SOCK`, `SIGNING`, `CERTIFICATE`).
- **Lexical Path Validation (`src/sandbox/path.rs`)**:
  - Added lexical path normalization resolving `.` and `..` before filesystem existence checks.

### ⚡ Critical Runtime Safety & LLM Protocol
- **UTF-8 Char Boundary Safety**:
  - Replaced unsafe byte slicing with `floor_char_boundary()` across output compactors (`src/tools/exec.rs`), web body truncation (`src/tools/web.rs`), system prompt truncation (`src/agent/prompt.rs`), API key masking (`src/ui/configure.rs`), and modal positioning (`src/ui/modal.rs`).
- **Tool Protocol Conformance (`src/agent/loop.rs`)**:
  - Fixed `Message::tool_result` to pass `tool_call.id` instead of `tool_call.name`.
- **Command Compactor Hardening (`src/tools/compactor.rs`)**:
  - Made `compact_cargo_check` and `compact_cargo_test` strictly preserve full error output whenever exit code is non-zero.
  - Made subcommand detection flag-aware (skipping flags like `+nightly`, `--quiet`, `-C`).
- **Process Safety (`src/tools/exec.rs`, `src/mcp/client.rs`)**:
  - Enabled `.kill_on_drop(true)` on `tokio::process::Command` to eliminate zombie child processes on timeout.
  - Added process group isolation (`process_group(0)`) on Unix for clean process tree teardowns.

### 📁 Filesystem & Patching Robustness
- **Empty File Read Support (`src/tools/fs.rs`)**:
  - Added explicit handling for 0-byte file reads without index out-of-bounds errors.
- **Atomic File Writes (`src/tools/fs.rs`, `src/context/memory.rs`)**:
  - All file writes now write to temporary sibling files (`.tmp_<pid>_<uuid>`) and atomically rename.
- **Sliding-Window Fuzzy Patch Matching (`src/tools/fs.rs`)**:
  - Replaced full-file diffing with sliding-window similarity scoring via `similar::TextDiff::ratio()`.
- **POSIX Trailing Newline Preservation (`src/tools/fs.rs`)**:
  - Preserved trailing newlines across all patch matching strategies.

### 🧠 AST Code Graph & Memory Engineering
- **Cross-File Dependency Graph (`src/context/graph.rs`)**:
  - Populated directed graph edges between modules based on extracted Tree-sitter symbol references.
- **Cross-Tier Memory Synchronization (`src/context/memory.rs`)**:
  - Synchronized and deduplicated preference keys across local and global memory stores.
- **Line-Anchored Task Plan Checkboxes (`src/context/working_memory.rs`)**:
  - Line-by-line matching for progress status updates without accidental substring replacements.
- **Float Standardization (`src/context/compressor.rs`)**:
  - Standardized threshold and margin math to `f64`.

### 🔄 Session Management & Multi-Turn Undo
- **Multi-Turn Consecutive `/undo` (`src/session/undo.rs`, `src/session/backup.rs`)**:
  - Canonicalized paths in backup checkpointing to seamlessly support relative paths.
  - Added automatic cleanup of rolled-back turn backup directories to allow consecutive undo operations.
- **Session Resume & Continue (`src/main.rs`, `src/app.rs`)**:
  - Wired CLI `--resume <session_id>` and `--continue` flags to hydrate previous session history into both interactive TUI and plain REPL modes.

### 🔌 Model Context Protocol (MCP) & UI Consistency
- **MCP Discovery & Compliance (`src/config.rs`, `src/mcp/server.rs`, `src/mcp/client.rs`)**:
  - Discovered both global (`~/.config/minicode/mcp.json`) and workspace (`.minicode/mcp.json`) MCP configurations.
  - Implemented standard JSON-RPC 2.0 error handling and response formatting.
- **UI Auto-Scroll Clamping (`src/ui/view.rs`)**:
  - Clamped manual and auto-scroll offsets to prevent viewport underflows and overflows.
- **Centralized Constants Documentation (`src/constants.rs`)**:
  - Documented all 45 constants with comprehensive rustdoc comments.

---

## [0.0.3] — 2026-08-14

### 🚀 Added
- **3D Retro-Futuristic Isometric Wordmark (`src/ui/view.rs`)**:
  - Implemented crisp 3D isometric block ASCII typography for the `minicode` hero intro banner in full Aura Theme colors.
  - Multi-tier color shading: Top highlights in Aura Purple (`#a277ff`), mid face in Pink (`#f694ff`), and base shadow in Mint Green (`#61ffca`).
- **Dynamic Git & Workspace Discovery (`src/ui/status.rs`, `src/ui/view.rs`)**:
  - Dynamic git branch detection returning `Option<String>` that gracefully displays active branch only when running inside an initialized git repository.
  - Dynamic `$HOME` path shortening (`~/...`) in both the welcome intro screen and bottom status bar.

### 🧹 Refactored & Polished
- **Zero-Hardcoding Enforcement**:
  - Replaced static version string literals with compile-time dynamic `env!("CARGO_PKG_VERSION")` and Clap's `#[command(version)]` attribute.
  - Modularized `TimelineContext` struct cleanly bundling theme, runtime timer, and workspace metadata without exceeding function argument limits.
  - Aligned repository attribution headers for OpenRouter API requests.

---

## [0.0.2] — 2026-08-14

### 🚀 Added
- **Dynamic Live Model Fetcher (`src/agent/models.rs`)**:
  - Automatically queries live provider API endpoints (OpenRouter `/api/v1/models`, Gemini `v1beta/models`, OpenAI-compatible `<base_url>/models`) to dynamically list all available models with zero hardcoding.
  - Automatically identifies free-tier models (`[FREE]`) and context window limits (`(Xk ctx)`).
  - Implemented local disk caching at `~/.config/minicode/models_cache.json` for instant UI loading and offline resilience.
- **In-TUI Interactive Modal System (`src/ui/modal.rs`)**:
  - Added floating Aura modal dialogs for switching providers and models mid-session (`/model`).
  - Added live fuzzy search filter in the model selector allowing developers to filter models as they type.
  - Full keyboard navigation with `↑`/`↓` arrows, `Enter` to select, and `Esc` to go back or close.
- **In-TUI Slash Command Recommendations & Arrow Navigation (`src/ui/input.rs`)**:
  - Real-time slash command recommendation popups (`/model`, `/provider`, `/undo`, `/clear`, `/help`, `/exit`) matching the reference UI design.
  - **`↑`/`↓` Arrow Navigation**: Interactive arrow key navigation across autocomplete recommendation rows with active `› ` indicator and elevated background styling.
  - **Instant Execution**: Pressing `Tab` or `Enter` immediately autocompletes or launches the highlighted slash command.
- **Enhanced Configuration Wizard (`src/ui/configure.rs`)**:
  - Hierarchical navigation with explicit back options (`[0] ◄ Back`, `b`, `Esc`) at every stage.
  - **Custom Provider Onboarding**: Support for adding custom OpenAI-compatible endpoints (vLLM, LM Studio, Ollama, LocalAI, etc.) with custom base URLs and live connection tests.
- **Safety Rollback Engine (`/undo`)**:
  - Seamless `/undo` command restoring previous turn backups directly inside the interactive TUI.

### 🛠️ Fixed
- Removed `tui_textarea` default cursor line underline from the input dock.
- Removed text underline from slash command recommendation highlights.
- Fixed UI event loop blocking during agent inference by introducing a non-blocking background Tokio actor task.

---

## [0.0.1] — 2026-08-14

### 🚀 Added
- **Dual-Mode Execution Architecture**:
  - **Interactive TUI Mode**: Full-screen Ratatui terminal application featuring a vertical streaming timeline, real-time token gauge, collapsible tool output folds, live execution timer, and multiline textarea input dock.
  - **Machine-Readable NDJSON IPC Mode (`--json-stream`)**: Bidirectional streaming over stdin/stdout for AI orchestrators, subagent swarms, and CI/CD pipelines.
  - **Accessible Plain REPL (`--plain`)**: Zero-alternate-screen scrolling terminal output.
- **Aura Theme Design System**:
  - Complete dark color palette inspired by Dalton Menezes' [Aura Theme](https://github.com/daltonmenezes/aura-theme) (`#15141b` dark background, `#29263c` elevated input container, `#a277ff` purple accent, `#61ffca` mint green success, `#ffca85` warm orange tool tags).
  - Clean, minimal inline timeline replacing cluttered multi-pane dashboards.
- **Interactive Configuration Wizard (`minicode configure`)**:
  - Step-by-step CLI wizard to select default providers, models, API keys, and execution approval policies.
  - Persistent settings stored in `~/.config/minicode/minicode.toml` and `.env`.
- **Universal Multi-Provider Engine**:
  - **OpenRouter Support**: Direct access to 100+ models (including Claude 3.5 Sonnet, DeepSeek-R1, Qwen2.5-Coder, Gemma, LFM) with free-tier compatibility and automatic exponential backoff retries on rate limits (HTTP 429).
  - **OpenAI & OpenAI-Compatible**: Support for OpenAI, DeepSeek, Groq, Together AI, and local vLLM/Ollama endpoints.
  - **Google Gemini**: Native reference provider with SSE streaming and `functionDeclarations`.
- **6 Core Coding Primitives**:
  - `read_file`: Line-ranged file inspection with boundary validation.
  - `write_file`: Atomic file writes with automatic parent directory creation.
  - `patch_file`: 3-stage search-and-replace block matching (exact match → whitespace normalization → fuzzy match via `similar`).
  - `exec_cmd`: Sandboxed shell execution with 30s timeout guard and 50KB output bounds.
  - `grep_search`: Fast regex codebase searching respecting `.gitignore` rules.
  - `fetch_or_browse`: HTML-to-Markdown scraper with Readability content extraction.
- **Security Sandbox Subsystem**:
  - Linux kernel Landlock filesystem isolation restricting file access strictly to the workspace root.
  - Landlock TCP network confinement by default for executed shell commands.
  - `env_clear()` environment sanitization with automatic API key/token stripping (`*_KEY`, `*_SECRET`, `*_TOKEN`).
  - Path canonicalization with symlink escape prevention.
- **AST Code Graph & Context Engine**:
  - Multi-language Tree-sitter AST queries for Rust, Python, JavaScript, and TypeScript with `mtime` caching.
  - Petgraph dependency graph builder with Personalized PageRank (biased toward active working files).
  - AST skeletonizer with token budget packing.
  - Exact BPE token counting via `tiktoken-rs` (cl100k_base / o200k_base).
  - Observation masking to truncate noisy tool outputs while preserving semantic head/tail context.
  - Dynamic `AGENTS.md` and workspace guideline discovery.
- **Session Persistence & Safety Rollback**:
  - Automatic turn-level file backup snapshots saved to `.minicode/backups/<turn_id>/` with JSON manifests.
  - `/undo` rollback engine restoring modified files and deleting created files.
  - Append-only JSONL session logging in `~/.config/minicode/sessions/`.
- **Low-Resource Developer Tooling**:
  - Developer `Justfile` with low-resource commands (`just check`, `just test`, `just clippy`, `just ci`, `just fmt`).
  - `.cargo/config.toml` hardware boundaries (`jobs = 2`, linker thread capping).
  - 23 unit tests verifying all core subsystems with 100% clean CI passes.
