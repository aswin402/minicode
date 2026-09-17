# Turbovec TurboQuant 1-Bit Vector Memory & Headroom DOX Ingress Observation Pruner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement high-density 1-bit/4-bit Turbovec TurboQuant vector compression with zero-copy memory mapping (`memmap2`) for 32x memory reduction and sub-millisecond AST code search, paired with a Headroom-inspired DOX observation ingress pruner that intercepts compiler warning cascades, passing test floods, and noisy tool outputs before they enter LLM context history.

**Architecture:**
- **Turbovec Engine:** 128-dimensional fixed-size binary vectors (`BinaryVector128`, 16 bytes) and 4-bit nibble quantized vectors (`PolarQuant4Fixed`, 64 bytes nibbles + 4 bytes scale) packed into flat contiguous 84-byte `TurbovecRecord` entries. Persisted into `.minicode/cache/semantic_index.bin` and searched via zero-copy `memmap2` with SIMD popcount Hamming distance and asymmetric dot products, eliminating JSON parsing and reducing memory by 32x.
- **Headroom DOX Pruner:** Pre-ingress observation sieves in `src/context/budget/log_pruner.rs` and `src/context/budget/observation_pruner.rs` that detect compiler warning cascades (collapsing 50+ warnings into a single badge while isolating critical errors) and test runner floods (collapsing passing tests into `✅ N passed`, preserving only failures). All pruned content is indexed into `CcrCache` for lossless on-demand recovery via `retrieve_observation`.

**Tech Stack:**
- Rust 2021 Edition, Tokio async runtime
- `memmap2` (pure-Rust memory mapping)
- Fast Walsh-Hadamard Transform (FWHT), SIMD popcount
- `CcrCache` (Compress-Cache-Retrieve), `thiserror`, `tracing`

## Global Constraints
- Always use `cargo test -j 1` and `cargo check -j 1` to respect system resource constraints.
- No `.unwrap()` or `.expect()` in library code (`src/context/`, `src/tools/`, `src/agent/`).
- Use `thiserror` for internal errors and `tracing` macros (`tracing::debug!`, `tracing::warn!`) instead of `println!`.
- Preserve lossless CCR retrieval: any pruned observation must be accessible via `retrieve_observation(id="...")`.

---

### Task 1: Fixed-Size Polarized Quantization (`PolarQuant4Fixed` and `TurbovecRecord`)

**Files:**
- Modify: `src/context/search/quantize.rs`
- Test: `src/context/search/quantize.rs` (inline unit tests)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
  pub struct PolarQuant4Fixed {
      pub nibbles: [u8; 64],
      pub scale: f32,
  }

  #[repr(C)]
  #[derive(Debug, Clone, Copy, PartialEq)]
  pub struct TurbovecRecord {
      pub binary: BinaryVector128,      // 16 bytes: 2 x u64
      pub quant4: PolarQuant4Fixed,     // 68 bytes: [u8; 64] + f32
  } // Total: 84 bytes, 0 heap allocations
  ```
- Consumes: `BinaryVector128`, `fwht_slice`

- [ ] **Step 1: Write the failing unit tests for `PolarQuant4Fixed` and `TurbovecRecord`**
  - Add tests in `src/context/search/quantize.rs` testing `PolarQuant4Fixed::from_f32_slice`, `asymmetric_dot_product`, and `TurbovecRecord` byte alignment (`std::mem::size_of::<TurbovecRecord>() == 84` or `std::mem::size_of::<PolarQuant4Fixed>() == 68`).
- [ ] **Step 2: Run `cargo test -j 1 --lib context::search::quantize::tests` and confirm failure**
- [ ] **Step 3: Implement `PolarQuant4Fixed` with zero heap allocation**
  - Use `[u8; 64]` instead of `Vec<u8>`.
  - Implement `asymmetric_dot_product(&self, query: &[f32]) -> f32`.
  - Implement `from_f32_slice(slice: &[f32]) -> Self`.
  - Implement `from_f32_with_wht(slice: &[f32]) -> Self`.
  - Define `TurbovecRecord` containing `BinaryVector128` and `PolarQuant4Fixed`.
- [ ] **Step 4: Run tests and verify they pass**
  - `cargo test -j 1 --lib context::search::quantize::tests`
- [ ] **Step 5: Git commit task changes**
  - `git commit -m "feat(context): add zero-heap PolarQuant4Fixed and TurbovecRecord"`

---

### Task 2: Flat Binary Vector Storage & `memmap2` Zero-Copy Index (`src/context/search/mmap_index.rs`)

**Files:**
- Create: `src/context/search/mmap_index.rs`
- Modify: `Cargo.toml` (add `memmap2 = "0.9"`)
- Modify: `src/context/search/mod.rs`
- Test: `src/context/search/mmap_index.rs` (inline unit tests)

**Interfaces:**
- Produces:
  ```rust
  pub struct MmapTurbovecIndex {
      mmap: Option<memmap2::Mmap>,
      fallback_data: Option<Vec<u8>>,
      record_count: usize,
      vector_offset: usize,
      string_pool_offset: usize,
      metadata_offset: usize,
  }

  impl MmapTurbovecIndex {
      pub fn open<P: AsRef<Path>>(path: P) -> Result<Self>;
      pub fn create<P: AsRef<Path>>(path: P, records: &[TurbovecRecord], metadata: &[ChunkMetadata]) -> Result<()>;
      pub fn search(&self, query_vec: &[f32], k: usize) -> Vec<(usize, f32)>;
      pub fn get_metadata(&self, index: usize) -> Option<ChunkMetadata>;
  }
  ```

- [ ] **Step 1: Add `memmap2 = "0.9"` to `Cargo.toml` and verify compilation**
  - Run `cargo check -j 1`.
- [ ] **Step 2: Write failing unit test for binary format serialization & mmap read**
  - Create `src/context/search/mmap_index.rs` with test `test_mmap_turbovec_roundtrip`.
  - Write sample records, save to temporary file, open with `MmapTurbovecIndex`, search top-K, and verify precision.
- [ ] **Step 3: Implement binary file header and record layout**
  - Header:
    - Magic: `*b"TURBOVEC\x02"` (8 bytes)
    - `u32` record count
    - `u32` vector dimension (128)
    - `u64` string pool byte offset
    - `u64` metadata table byte offset
  - Followed by contiguous records: `record_count * 84` bytes.
  - Followed by string pool and metadata offset entries.
- [ ] **Step 4: Implement 2-stage search kernel**
  - Stage 1: Iterate over memory-mapped `BinaryVector128` slice. Fast SIMD popcount Hamming distance into top candidates.
  - Stage 2: For top candidates, execute `asymmetric_dot_product` on `PolarQuant4Fixed` slice directly from mmap memory.
  - Return top-K `(chunk_index, similarity_score)`.
- [ ] **Step 5: Run tests and ensure all pass**
  - `cargo test -j 1 --lib context::search::mmap_index::tests`
- [ ] **Step 6: Git commit task changes**
  - `git commit -m "feat(context): implement zero-copy MmapTurbovecIndex with 2-stage search"`

---

### Task 3: Upgrade `SemanticIndex` to Use Mmap Turbovec Storage

**Files:**
- Modify: `src/context/search/semantic.rs`
- Test: `tests/integration_context_engine_v2.rs`

**Interfaces:**
- Updates `SemanticIndex::build_index`, `SemanticIndex::save_cache`, and `SemanticIndex::load_cache` to write `.minicode/cache/semantic_index.bin`.
- Preserves automatic migration: if `.minicode/cache/semantic_index.json` exists, load it, save as `.bin`, and clean up `.json`.

- [ ] **Step 1: Write test for legacy JSON migration and binary index loading**
- [ ] **Step 2: Update `CodeChunk` to use `PolarQuant4Fixed`**
  - Replace `Option<PolarQuant4>` with `Option<PolarQuant4Fixed>`.
  - Update `chunk_source_code_ast` and `chunk_source_code_sliding` to instantiate `PolarQuant4Fixed` and `BinaryVector128`.
- [ ] **Step 3: Update `save_cache` and `load_cache` in `SemanticIndex`**
  - Save to `.minicode/cache/semantic_index.bin` using `MmapTurbovecIndex::create`.
  - Load via `MmapTurbovecIndex::open`.
  - If `.bin` is missing but `.json` exists, migrate smoothly.
- [ ] **Step 4: Accelerate `search_fast` and `search_symbols`**
  - When mmap index is active, delegate search to `MmapTurbovecIndex::search`.
- [ ] **Step 5: Run cargo tests to verify zero regressions**
  - `cargo test -j 1 --lib context::search::semantic::tests`
- [ ] **Step 6: Git commit task changes**
  - `git commit -m "feat(context): wire MmapTurbovecIndex into SemanticIndex with zero-copy search"`

---

### Task 4: Headroom Compiler Warning Cascade Sieve (`LogPruner`)

**Files:**
- Modify: `src/context/budget/log_pruner.rs`
- Test: `src/context/budget/log_pruner.rs` (inline unit tests)

**Interfaces:**
- Produces:
  ```rust
  impl LogPruner {
      pub fn condense_compiler_warnings(lines: &[&str]) -> (Vec<String>, usize);
  }
  ```
- Behavior:
  - If a log contains $\ge 3$ compiler warnings (e.g. `warning: ...`, `warning TS...`, `warn: ...`) alongside errors or successes:
  - Condense all warning bodies into a clean single-line badge:
    `⚠️  [42 compiler warnings collapsed — original cached in CCR]`
  - Strictly preserve every compiler error (`error[E...]`, `error TS...`, `SyntaxError`, `panic!`) and the final build status intact!

- [ ] **Step 1: Write failing unit test `test_condense_compiler_warnings`**
  - Test a 200-line compiler output with 30 warnings and 1 fatal error `error[E0382]`.
  - Assert that warnings are collapsed into badge, and `error[E0382]` is fully retained with line numbers.
- [ ] **Step 2: Run `cargo test -j 1 --lib context::budget::log_pruner::tests` and observe failure**
- [ ] **Step 3: Implement `condense_compiler_warnings` in `LogPruner`**
  - Parse warning headers and diagnostic bodies.
  - When warning count exceeds threshold (3), summarize warnings and keep errors.
- [ ] **Step 4: Integrate warning condensation into `LogPruner::prune`**
- [ ] **Step 5: Run tests and verify they pass**
  - `cargo test -j 1 --lib context::budget::log_pruner::tests`
- [ ] **Step 6: Git commit task changes**
  - `git commit -m "feat(budget): add compiler warning cascade sieve to LogPruner"`

---

### Task 5: Headroom Test Runner Output Condenser (`LogPruner`)

**Files:**
- Modify: `src/context/budget/log_pruner.rs`
- Test: `src/context/budget/log_pruner.rs` (inline unit tests)

**Interfaces:**
- Produces:
  ```rust
  impl LogPruner {
      pub fn condense_test_runner_output(lines: &[&str]) -> (Vec<String>, usize);
  }
  ```
- Behavior:
  - Sieve for `cargo test`, `pytest`, `jest`, `bun test`, `vitest`, `go test`:
  - When $\ge 5$ passing test lines occur (e.g. `test foo ... ok`, `PASS src/foo.test.ts`):
  - Collapse them into: `✅  [N passing tests collapsed]`.
  - If any test failed (`test bar ... FAILED`, `FAIL src/bar.test.ts`, assertions):
  - Retain the failing test names, assertion diffs, and failures summary verbatim!

- [ ] **Step 1: Write failing unit test `test_condense_test_runner_output`**
  - Test a 150-line `cargo test` run with 148 passed tests and 1 failed test.
  - Verify that the 148 passed tests are collapsed, while the failure assertion and summary are preserved.
- [ ] **Step 2: Run `cargo test -j 1 --lib context::budget::log_pruner::tests` and observe failure**
- [ ] **Step 3: Implement `condense_test_runner_output` in `LogPruner`**
  - Detect test framework pass/fail patterns.
  - Collapse consecutive pass lines.
- [ ] **Step 4: Wire test runner condensation into `LogPruner::prune`**
- [ ] **Step 5: Run tests and verify they pass**
  - `cargo test -j 1 --lib context::budget::log_pruner::tests`
- [ ] **Step 6: Git commit task changes**
  - `git commit -m "feat(budget): add test runner output condenser to LogPruner"`

---

### Task 6: Unified Pre-Ingress Observation Sieves in `ObservationPruner` & `AgentLoop`

**Files:**
- Modify: `src/context/budget/observation_pruner.rs`
- Modify: `src/agent/loop.rs`
- Test: `tests/integration_context_engine_v2.rs`

**Interfaces:**
- Wire `ObservationPruner::prune_for_llm` at every tool output ingress point:
  - Sequential tool execution in `AgentLoop`
  - Parallel batch tool execution in `AgentLoop`
  - Background subagent execution in `SubagentWorker`
- Ensure every compressed observation includes `[CCR Ref: ccr_<hash>]` and is indexed in `CcrCache`.

- [ ] **Step 1: Write integration test verifying observation pruning across tool results in `AgentLoop`**
- [ ] **Step 2: Update `ObservationPruner::prune_for_llm` to route compiler outputs and test outputs to the enhanced `LogPruner`**
- [ ] **Step 3: Verify that `retrieve_observation` can seamlessly reconstruct the full raw output**
- [ ] **Step 4: Run integration tests**
  - `cargo test -j 1 --test integration_context_engine_v2`
- [ ] **Step 5: Git commit task changes**
  - `git commit -m "feat(agent): wire unified pre-ingress observation sieves across tool execution"`

---

### Task 7: Full Verification, Quality Gates & Release Deployment (`v0.3.27`)

**Files:**
- Modify: `Cargo.toml` (bump version → `0.3.27`)
- Modify: `onpkg_docs/todo.md` (record Phase 126 and Phase 127)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Run comprehensive test suite**
  - `cargo test -j 1 --bin minicode`
  - `cargo test -j 1 --lib`
- [ ] **Step 2: Run linter and formatting checks**
  - `cargo clippy -j 1 -- -D warnings`
  - `cargo fmt --check`
- [ ] **Step 3: Update docs and bump version in `Cargo.toml` to `0.3.27`**
- [ ] **Step 4: Run `./localupdate.sh` to compile release binary and install globally to `~/.local/bin/minicode`**
- [ ] **Step 5: Verify global binary with `minicode --version`**
- [ ] **Step 6: Git commit all updates**
