---
name: "open-code-review"
description: "Hybrid precision code review workflow combining deterministic filtering, secret-path guards, language & repo rules (.minicode/review_rules.json or .opencodereview/rule.json), AST-clustered parallel inspection, exact line anchoring, and behavioral impact analysis."
category: "quality-review"
kind: "workflow"
globs: ["**/*"]
triggers: ["code review", "review", "pr review", "audit diff", "open-code-review", "ocr", "sarif"]
always_apply: false
---

# Open Code Review — Native Precision Review Workflow

> Adapted for `minicode`'s native pure-Rust Hybrid Review Engine (`src/git/ocr/`, `power_review`, and `minicode review`), inspired by `alibaba/open-code-review` (Apache-2.0).

## Core Principle

**Deterministic code decides *what* gets reviewed and *where* comments land; the LLM decides *what is wrong*.**

Favor **precision over recall**: report only defects that are likely real in the changed code and its reachable context. Every false positive erodes reviewer trust. Never repeat diagnostics that `rustc`, `clippy`, `tsc`, `eslint`, or `rustfmt` already catch unless the diff creates a concrete runtime or security bug.

---

## Workflow

### Step 1 — Gather Concise Business Context
Before reviewing, distill the user's goal, `.minicode/decisions.md`, or commit/PR description into 1–3 sentences of business context and pass it via `power_review(background="...")` (or `minicode review --background "..."`).

### Step 2 — Zero-Token Preview & Scope Verification (Optional on Large Diffs)
For large branches or unfamiliar repos, run `power_review(preview=true)` (or `minicode review --preview`) first:
- Confirm all intended source files appear in `reviewable_files`.
- Verify secret/credential paths (`.env`, `*.pem`, `*.key`, credentials) and lockfiles were deterministically excluded.
- Check whether `.minicode/review_rules.json` or `.opencodereview/rule.json` custom path rules are active.

### Step 3 — Run Native Hybrid Review
- **Workspace changes (default):**
  ```
  power_review(background="<concise context>", analysis=true)
  ```
- **Commit / branch range (`base_sha..head_sha`):**
  ```
  power_review(base_sha="main", head_sha="HEAD", background="<context>", analysis=true)
  ```
- **Standalone CLI / CI SARIF export:**
  ```bash
  minicode review --preview
  minicode review --from main --to HEAD --audience agent --format json
  minicode review --from main --to HEAD --format sarif > review.sarif
  minicode review --scan --audience human
  ```

### Step 4 — Mandatory 100% Coverage & Evidence Grounding
- Every file in the review cluster must be inspected or explicitly noted as skipped — never silently omit smaller config, header, or test files.
- Before claiming a cross-file bug, race condition, or security flaw, verify callers, types, and boundaries with `read_file` or `grep_search`.
- Anchor every defect to an exact verified `path:line` coordinate.

### Step 5 — Asymmetric Precision Triage
Categorize findings strictly by real user/production impact:
- **`CRITICAL` / `HIGH` (Blocking):** Crashes/panics in production paths, data loss/corruption, security vulnerabilities (SQLi, XSS, injection, auth bypass, secret exposure), concurrency/async deadlocks, broken public contracts.
- **`MEDIUM` (Blocking if real):** Edge-case correctness defects, stale state/cache bugs, missing input validation on untrusted boundaries, broken tests.
- **`LOW` (Non-blocking):** Minor maintainability improvements — defer unless trivial.
- **Discard silently:** Speculative worries, personal style preferences, or already-enforced linter nits.

### Step 6 — Separate Behavior & Impact Brief
When `analysis=true` (or reviewing a branch/PR/commit), report **material behavioral changes** separately from defect findings — even when there are 0 bugs (e.g., removed CLI flags, changed defaults, new storage schema keys, changed error codes). Cite exact `file:line` evidence and omit the section if there is no observable behavioral shift.

### Step 7 — Targeted Fix Loop (Only When Requested)
If the user asked to review **and fix**:
1. Fix every `CRITICAL` / `HIGH` / concrete `MEDIUM` defect with covering tests.
2. Run `power_verify` (all 4 gates including Gate 2 test suite) and one concise `power_review(stage="re-review", fix_round=1)`.
3. Stop cleanly once static audit + verification pass — do not enter endless style-polishing loops.
