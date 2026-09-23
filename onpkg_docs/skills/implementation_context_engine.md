# Phase 119: Next-Gen Context Compression, KV-Cache Stability & High-Density Memory Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform `minicode` into the fastest, leanest, and most context-efficient Rust-native AI coding agent by eliminating KV-cache prefix invalidation, introducing Headroom-MCP structural JSON and CCR observation pruning, incorporating Hermes dual-layer thought ergonomics and orphan-safe compaction, consolidating memory systems, and integrating Turbovec 4-bit polarized quantized vector search.

**Architecture:** 
1. Strict Tri-Zone immutability preserving byte-for-byte KV prefix stability across turns.
2. Ingress observation pruning (SmartCrusher + Run-length log dedup) with reversible local caching (CCR).
3. Dual-layer reasoning persistence (raw thoughts stored for provider compliance, stripped from older history to eliminate $O(N^2)$ bloat).
4. Unified Progressive Memory (L0–L3) cached in RAM with biological half-life decay.
5. High-density 4-bit PolarQuant SIMD vector search (16 bytes/chunk) for instant in-process code and memory retrieval.

**Tech Stack:** Rust 2021 Edition, Tokio async runtime, Ratatui, tiktoken-rs, tree-sitter, petgraph, simsimd/packed-simd bitwise popcount.

## Global Constraints
- Strictly pure Rust with zero mandatory external C or daemon dependencies.
- Maintain `cargo check -j 1` and `cargo test -j 1` concurrency limits.
- Zero `.unwrap()` or `.expect()` calls in library code (`src/`); use `thiserror` and `Result<T>`.
- Zero `println!` or `eprintln!` in library code; use `tracing::*`.
- Preserve backward compatibility for all 132 existing agent tools.

---

## File Structure & Responsibilities

| File | Status | Primary Responsibility |
|---|---|---|
| `src/agent/loop.rs` | Modify | Stop in-place mutation of historical user messages; sort tools deterministically; in-memory progressive memory cache. |
| `src/agent/prompt.rs` | Modify | Remove double-nested XML tags; isolate volatile recency context to prompt tail; enforce byte-stable Zone 1. |
| `src/agent/types.rs` | Modify | Add `reasoning_content: Option<String>` to `Message`; add KV-cache metrics to `StreamChunk::Usage`. |
| `src/agent/providers/anthropic.rs` | Modify | Add `cache_control: {"type": "ephemeral"}` breakpoints; parse `cache_read_input_tokens`. |
| `src/agent/providers/openai.rs` | Modify | Parse `prompt_tokens_details.cached_tokens`; add inline `<tool_call>` XML stream parser fallback. |
| `src/context/budget/auto_compact.rs` | Modify | Add orphan-safe cutoff alignment; strip reasoning blocks older than 2 turns; anti-thrashing check. |
| `src/context/budget/json_crusher.rs` | Create | `SmartCrusher` structural JSON array compressor with constant factoring and Kneedle sampling. |
| `src/context/budget/ccr_cache.rs` | Create | Reversible Compress-Cache-Retrieve (CCR) local storage for truncated observations. |
| `src/context/governance/dox.rs` | Create | Hierarchical `AGENTS.md` path-scoped rule resolution (root -> crate -> module). |
| `src/tools/registry/context_tools.rs` | Modify | Register `retrieve_observation` tool providing lossless access to CCR-cached outputs. |
| `src/context/memory/progressive_memory.rs` | Modify | Consolidate `CoreMemory` into `ProgressiveMemory`; connect biological decay engine. |
| `src/context/search/quantize.rs` | Create | 4-bit Polarized Quantization (TurboQuant) packing 128 dimensions into 16 bytes with SIMD popcount. |
| `src/context/search/semantic.rs` | Modify | Upgrade `SemanticIndex` to use `BinaryVector128` and in-kernel slot masking. |
| `src/ui/view.rs` & `src/ui/status.rs` | Modify | Render KV cache hit rate in TUI status line (`92% cached • TTFT: 140ms`); collapsible thought badges. |
| `tests/integration_context_engine_v2.rs` | Create | End-to-end integration tests for KV prefix stability, CCR retrieval, JSON crushing, and quantized search. |

---

## Tasks

### Task 1: KV-Cache Prefix Stabilization & Provider Cache Directives (LMCache & Anthropic Pattern)

**Files:**
- Modify: `src/agent/types.rs:40-120`
- Modify: `src/agent/loop.rs:270-325`
- Modify: `src/agent/prompt.rs:200-270`
- Modify: `src/agent/providers/mod.rs:50-80`
- Modify: `src/agent/providers/anthropic.rs:180-250`
- Modify: `src/agent/providers/openai.rs:190-260`
- Test: `tests/integration_context_engine_v2.rs`

**Interfaces:**
- Produces: `Message.reasoning_content`, `StreamChunk::Usage.cached_prompt_tokens`
- Consumes: `PromptBuilder::build_static_system_prompt`, `PromptBuilder::build_recency_context`

- [ ] **Step 1: Write failing test for immutable historical messages and KV prefix stability**
  Assert that consecutive turns share an exact, byte-for-byte identical prefix for turns $0 \dots N-1$, and verify that `sanitize_past_user_message` does not mutate past message contents in `self.messages`.

- [ ] **Step 2: Update `src/agent/types.rs` with cache telemetry and reasoning content**
  Add `pub cached_prompt_tokens: usize` to `StreamChunk::Usage` and `pub reasoning_content: Option<String>` to `Message`.

- [ ] **Step 3: Eliminate past user message mutation in `src/agent/loop.rs`**
  Remove the in-place loop modifying past `Role::User` messages. Ensure that dynamic recency context (`<workspace_context>`) is only appended to the trailing user request for the active turn and is never written into the historical message list.

- [ ] **Step 4: Deterministic Tool Sorting**
  Sort tools alphabetically by name (`tools.sort_by(|a, b| a.name.cmp(&b.name))`) before passing them to provider payloads, preventing tool schema order jitter across restarts.

- [ ] **Step 5: Inject Anthropic `cache_control` breakpoints in `anthropic.rs`**
  Add `cache_control: {"type": "ephemeral"}` to:
  1. The static system prompt block.
  2. The final entry in the `tools` array.
  3. The penultimate user message (if total context > 1,024 tokens).
  Parse `cache_read_input_tokens` and `cache_creation_input_tokens` into `StreamChunk::Usage`.

- [ ] **Step 6: Parse cached tokens in `openai.rs`**
  Extract `prompt_tokens_details.cached_tokens` from OpenAI/DeepSeek/vLLM usage JSON and forward via `StreamChunk::Usage`.

- [ ] **Step 7: Run tests to verify Task 1 passes**
  `cargo test test_kv_prefix_stability -j 1`

---

### Task 2: Headroom-MCP Ingress Pruning, Reversible CCR & DOX Scoping

**Files:**
- Create: `src/context/budget/json_crusher.rs`
- Create: `src/context/budget/ccr_cache.rs`
- Create: `src/context/governance/dox.rs`
- Modify: `src/tools/registry/context_tools.rs`
- Modify: `src/tools/execution.rs`
- Test: `tests/integration_context_engine_v2.rs`

**Interfaces:**
- Produces: `JsonCrusher::crush_json_array`, `CcrCache::store`, `CcrCache::retrieve`, `DoxEngine::resolve_scoped_rules`
- Consumes: `ToolResult.output`, `ToolRegistry::dispatch`

- [ ] **Step 1: Write failing test for `JsonCrusher` and `CcrCache`**
  Provide a 50-item JSON array, verify constant keys are factored into `_common_fields`, errors are preserved, and a valid `ccr_xxxx` hash is returned. Verify `CcrCache::retrieve` returns the original uncompressed slice.

- [ ] **Step 2: Implement `src/context/budget/json_crusher.rs`**
  Implement `SmartCrusher` extracting common fields, preserving head (first 2 items), tail (last 2 items), 100% of error/failure entries, and sampling homogeneous middle items with an attached CCR reference.

- [ ] **Step 3: Implement `src/context/budget/ccr_cache.rs`**
  Implement a bounded in-memory LRU cache (`CcrCache`, max 50MB) storing raw outputs keyed by SHA-256 slice hashes (`ccr_xxxx`).

- [ ] **Step 4: Register `retrieve_observation` tool in `context_tools.rs`**
  Add tool schema and dispatch for `retrieve_observation(id: String, offset: Option<usize>, limit: Option<usize>)`, giving the agent a lossless retrieval escape hatch.

- [ ] **Step 5: Implement Hierarchical DOX Scoping in `src/context/governance/dox.rs`**
  Traverse path hierarchy from workspace root to active files, reading localized `AGENTS.md` and aggregating them without duplicating root invariants.

- [ ] **Step 6: Run tests to verify Task 2 passes**
  `cargo test test_json_crusher_and_ccr -j 1`

---

### Task 3: Hermes Thought Ergonomics & Tier 2 Compaction Safety

**Files:**
- Modify: `src/context/budget/auto_compact.rs:400-520`
- Modify: `src/agent/loop.rs:720-760`
- Modify: `src/agent/providers/openai.rs:120-170`
- Test: `tests/integration_context_engine_v2.rs`

**Interfaces:**
- Produces: Safe interaction boundary alignment in `AutoCompactor::compact`
- Consumes: `Message.role`, `Message.tool_calls`

- [ ] **Step 1: Write failing test for Tier 2 orphan tool result avoidance**
  Construct a message chain where slicing at `cutoff` lands on a `Role::Tool` message. Verify that compaction snaps `cutoff` backward so that tool calls and tool responses are never separated.

- [ ] **Step 2: Fix cutoff boundary alignment in `auto_compact.rs`**
  Add alignment logic:
  ```rust
  while cutoff > 0 && messages[cutoff].role == Role::Tool {
      cutoff -= 1;
  }
  ```
  Ensures `recent_slice` never starts with an orphaned tool result, eliminating HTTP 400 Bad Request errors.

- [ ] **Step 3: Implement older thought stripping during compaction**
  In `AutoCompactor::compact`, strip `<thought>` / `<think>` and `reasoning_content` from all turns older than the 2 most recent turns, eliminating quadratic $O(N^2)$ token creep.

- [ ] **Step 4: Add fallback inline XML `<tool_call>` parser in `openai.rs`**
  Add a streaming buffer that detects `<tool_call>{"name": ..., "arguments": ...}</tool_call>` in `delta.content` when using local models (Hermes 3, Qwen) that omit SSE `delta.tool_calls`.

- [ ] **Step 5: Run tests to verify Task 3 passes**
  `cargo test test_compaction_orphan_safety -j 1`

---

### Task 4: Memory Dual-Stack Consolidation & Zero-IO Hot Path

**Files:**
- Modify: `src/agent/prompt.rs:240-270`
- Modify: `src/context/memory/progressive_memory.rs:1-120`
- Modify: `src/context/memory/decay.rs:50-140`
- Modify: `src/agent/loop.rs:180-220`
- Test: `tests/integration_context_engine_v2.rs`

**Interfaces:**
- Produces: `ProgressiveMemoryManager` with in-memory caching and biological decay
- Deprecates: Redundant `CoreMemory` in `src/context/memory/memory.rs`

- [ ] **Step 1: Write failing test for XML tag duplication and memory decay**
  Verify that `build_recency_context` produces non-nested `<progressive_memory>` tags and verify that transient memory facts decay with time while permanent facts retain 1.0 strength.

- [ ] **Step 2: Fix double-nested XML in `src/agent/prompt.rs`**
  Remove redundant `<progressive_memory>` wrapper tags in `build_recency_context` that enclose blocks that already format their own root tags.

- [ ] **Step 3: Consolidate `CoreMemory` into `ProgressiveMemory`**
  Migrate all `CoreMemory` keys (local/global facts) into `ProgressiveMemory` tiers (L2 project facts, L3 global preferences). Remove duplicate disk reads.

- [ ] **Step 4: In-Memory Caching in `AgentLoop`**
  Store an `Arc<RwLock<ProgressiveMemory>>` in `AgentLoop`. Load once at workspace initialization, save only on dirty mutations, eliminating 5+ synchronous disk reads from the turn hot path.

- [ ] **Step 5: Wire biological decay in `decay.rs`**
  Connect `CognitiveMemoryManager` so transient debug findings decay with a 60-minute half-life while permanent invariants remain undecayed.

- [ ] **Step 6: Run tests to verify Task 4 passes**
  `cargo test test_memory_consolidation_and_decay -j 1`

---

### Task 5: Turbovec High-Density Quantized Vector Search Engine & Quality Gates

**Files:**
- Create: `src/context/search/quantize.rs`
- Modify: `src/context/search/semantic.rs:1-150`
- Modify: `src/ui/status.rs:60-120`
- Modify: `src/ui/view.rs:220-280`
- Create: `tests/integration_context_engine_v2.rs`
- Modify: `Cargo.toml`, `CHANGELOG.md`

**Interfaces:**
- Produces: `BinaryVector128`, `TurboQuantIndex`, SIMD popcount cosine similarity
- Consumes: `SemanticIndex::search`, `SemanticIndex::add_chunk`

- [ ] **Step 1: Write failing test for 4-bit / 1-bit Polar Quantization**
  Generate 128-dimensional dense float vectors, quantize into `BinaryVector128` (16 bytes), compute hamming distance similarity, and verify $\ge 92\%$ rank correlation with full FP32 cosine similarity.

- [ ] **Step 2: Implement `src/context/search/quantize.rs`**
  Implement `BinaryVector128` packing 128 float signs into two `u64` values (`[u64; 2]`) with SIMD popcount hamming distance (`bits[0] ^ other.bits[0]).count_ones()`.

- [ ] **Step 3: Upgrade `src/context/search/semantic.rs`**
  Replace uncompressed `Vec<f32>` with `BinaryVector128`, cutting chunk memory footprint from 512 bytes down to 16 bytes (32x memory reduction). Add in-kernel path prefix slot masking.

- [ ] **Step 4: Render KV Cache Telemetry in TUI Status Line**
  In `src/ui/status.rs`, display cache efficiency:
  `Context: 42.1k (88% cached • TTFT: 160ms)`

- [ ] **Step 5: Comprehensive Integration Test Suite**
  Write end-to-end scenarios in `tests/integration_context_engine_v2.rs` exercising KV-cache prefix stability, JSON crushing, CCR recovery, orphan-safe compaction, and quantized vector search.

- [ ] **Step 6: Execute Quality Gates & Release v0.3.19**
  - Run `cargo check -j 1`
  - Run `cargo test --bin minicode -j 1`
  - Run `cargo test --test integration_context_engine_v2 -j 1`
  - Run `cargo clippy -j 1 -- -D warnings`
  - Run `cargo fmt --check`
  - Update `CHANGELOG.md` and bump `version = "0.3.19"` in `Cargo.toml`
  - Run `./localupdate.sh`
  - Git commit: `git commit -m "feat(context): Next-Gen Context Compression, KV-Cache Stability & High-Density Memory Engine (Phase 119)"`
