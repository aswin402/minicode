//! Embedded built-in curated skills catalog for MiniVault (Tier 1).

use crate::vault::models::{parse_skill_markdown, SkillScope, VaultBundle, VaultSkill};

pub struct BuiltinSkillDef {
    pub name: &'static str,
    pub content: &'static str,
}

pub const BUILTIN_SKILLS: &[BuiltinSkillDef] = &[
    BuiltinSkillDef {
        name: "tailwind-v4",
        content: r#"---
name: "tailwind-v4"
description: "Modern Tailwind CSS v4 styling rules, container queries, and CSS-first configuration."
category: "ui-styling"
kind: "reference"
globs: ["*.css", "*.tsx", "*.jsx", "*.html", "*.vue", "*.svelte"]
triggers: ["tailwind", "css", "styling", "ui design", "responsive"]
always_apply: false
---

# Tailwind CSS v4 Guidelines

## Invariants & Rules
1. **CSS-First Configuration**: In Tailwind v4, `@import "tailwindcss";` is used in root CSS. Do NOT generate legacy `tailwind.config.js` unless requested.
2. **Design Tokens & Theme Variables**: Define custom theme properties directly via `@theme { --color-brand: #...; }` or standard CSS custom properties.
3. **Container Queries**: Prefer container queries (`@container`, `@md:grid-cols-2`) for self-contained modular components rather than global viewport breakpoints alone.
4. **Modern Utility Combinations**:
   - Spacing: Prefer gap (`gap-4`, `gap-6`) over individual margins when using flex or grid.
   - Text hierarchy: Combine tracking (`tracking-tight`), line height (`leading-relaxed`), and muted foregrounds (`text-neutral-500`, `dark:text-neutral-400`).
   - Surfaces: Layer surfaces cleanly with `bg-background`, `border border-neutral-200 dark:border-neutral-800`, and subtle shadows (`shadow-sm`, `shadow-md`).
"#,
    },
    BuiltinSkillDef {
        name: "frontend-design",
        content: include_str!("../tools/minikit/builtin_skills/frontend-design/SKILL.md"),
    },
    BuiltinSkillDef {
        name: "ui-ux-pro-max",
        content: include_str!("../tools/minikit/builtin_skills/ui-ux-pro-max/SKILL.md"),
    },
    BuiltinSkillDef {
        name: "react",
        content: r#"---
name: "react"
description: "Modern React 19 patterns, Server Components, hooks, custom hooks, and state management."
category: "frontend"
kind: "reference"
globs: ["*.tsx", "*.jsx", "src/**/*.ts"]
triggers: ["react", "component", "hook", "useState", "useEffect"]
always_apply: false
---

# React Engineering Standards

## Invariants
1. **Functional Components**: Use pure functional components with strict TypeScript types for props.
2. **Hooks Hygiene**:
   - Never call hooks conditionally.
   - Specify exhaustive dependencies in `useEffect`, `useCallback`, and `useMemo`.
   - Do NOT use `useEffect` for state synchronization that can be derived during render.
   - Never call `setState` synchronously inside a `useEffect` body (violates `eslint-plugin-react-hooks` v7 `react-hooks/set-state-in-effect`); initialize state lazily via `useState(() => ...)` or only call `setState` inside async/timer/observer/event callbacks.
3. **State Co-location**: Keep state as close as possible to where it is consumed. Avoid premature global state.
4. **Error Boundaries**: Wrap asynchronous or error-prone components in standard Error Boundaries with fallback UIs.
"#,
    },
    BuiltinSkillDef {
        name: "nextjs",
        content: r#"---
name: "nextjs"
description: "Next.js App Router, React Server Components (RSC), Server Actions, metadata, and routing."
category: "frontend"
kind: "reference"
globs: ["app/**/*.tsx", "app/**/*.ts", "next.config.*"]
triggers: ["nextjs", "next", "next.js", "app router", "server actions", "rsc"]
always_apply: false
---

# Next.js App Router Architecture

## Invariants
1. **Server First**: Components are Server Components by default. Add `'use client'` strictly at leaf nodes that require client-side interactivity, DOM listeners, or React hooks.
2. **Server Actions**: Define server actions with `'use server'` and validate all inputs with Zod/Pydantic schemas before executing database queries.
3. **SEO & Metadata**: Export static or dynamic `generateMetadata` for every page route.
4. **Caching & Revalidation**: Use `revalidatePath` and `revalidateTag` deliberately to invalidate cached server content.
"#,
    },
    BuiltinSkillDef {
        name: "rust-tokio",
        content: r#"---
name: "rust-tokio"
description: "Systems Rust, Tokio multi-threaded runtime, cost-model-first optimization, cache-friendly data layout, and zero-panic error handling."
category: "runtime"
kind: "runtime"
globs: ["*.rs", "Cargo.toml"]
triggers: ["rust", "tokio", "async rust", "cargo", "channel", "btree", "performance"]
always_apply: false
---

# Systems & Async Rust Engineering Standards

## Core Invariants
1. **Zero Panics**: Never use `.unwrap()` or `.expect()` in non-test production code. Return `Result<T, E>` with `thiserror` for library code and `anyhow` at CLI boundaries.
2. **Deep Logic, Thin Interface**: Write code that requires minimal human attention. Encapsulate complex invariants behind small, obvious APIs. Functions should belong as methods on a clear owner `struct` or `enum` rather than free-floating functions.
3. **Cost-Model First, Benchmark Last**: Reason through the hardware/memory cost model (allocations, cache lines, asymptotic complexity) before optimizing. Benchmarks are the final step to verify your cost model, not a tool for blind trial-and-error optimization.
4. **Data Layout Over Micro-Hacks**: Fix memory and lookup latency via contiguous cache-friendly data layout, inline key prefixes (`u64::from_be_bytes`), and ordered B-Tree range scans (`BTreeMap::range`) rather than $O(N)$ heap string scans or software prefetching.
5. **Cancellation Safety & Bounded Channels**: Ensure all `tokio::select!` branches are cancellation safe. Always prefer bounded channels (`tokio::sync::mpsc::channel(N)`) over unbounded channels to prevent memory leaks under backpressure.
6. **Blocking Work**: Offload CPU-heavy or blocking filesystem operations to `tokio::task::spawn_blocking`.
7. **Living Decision Ledger (`D1..DN`)**: Record architectural invariants as numbered entries (`[D1]`, `[D2]`, ...) via `record_decision` in `.minicode/decisions.md` and update them in place when decisions evolve.
"#,
    },
    BuiltinSkillDef {
        name: "fastapi",
        content: r#"---
name: "fastapi"
description: "Python FastAPI development with Pydantic v2, async endpoints, dependency injection, and OpenAPI."
category: "backend"
kind: "reference"
globs: ["*.py", "requirements.txt", "pyproject.toml"]
triggers: ["fastapi", "python", "pydantic", "uvicorn"]
always_apply: false
---

# FastAPI Best Practices

## Invariants
1. **Pydantic v2 Models**: Use Pydantic BaseModel for request validation and response schemas.
2. **Dependency Injection**: Use `Depends(...)` for database sessions, authentication, and service locators.
3. **Async Endpoints**: Use `async def` for I/O-bound operations and standard `def` for synchronous CPU tasks to prevent event loop starvation.
4. **HTTP Status Codes**: Explicitly set appropriate `status_code` (`status.HTTP_201_CREATED`, `status.HTTP_204_NO_CONTENT`).
"#,
    },
    BuiltinSkillDef {
        name: "postgres",
        content: r#"---
name: "postgres"
description: "PostgreSQL database schemas, indexing, migrations, JSONB queries, and connection pooling."
category: "database"
kind: "reference"
globs: ["*.sql", "prisma/schema.prisma", "migrations/**/*.sql"]
triggers: ["postgres", "postgresql", "sql", "migration", "database"]
always_apply: false
---

# PostgreSQL Standards

## Invariants
1. **Primary Keys**: Prefer UUIDv7 or `BIGINT GENERATED ALWAYS AS IDENTITY` for scalable primary keys.
2. **Explicit Indexes**: Add indexes for all foreign key columns and frequently filtered columns.
3. **Transactions**: Wrap multi-table state modifications in explicit transactions (`BEGIN ... COMMIT`) with rollback on failure.
4. **Parameterized Queries**: Never concatenate raw strings into SQL queries. Always use parameterized statements to eliminate SQL injection.
"#,
    },
    BuiltinSkillDef {
        name: "docker",
        content: r#"---
name: "docker"
description: "Production Dockerfiles, multi-stage builds, non-root users, compose files, and container optimization."
category: "devops"
kind: "reference"
globs: ["Dockerfile*", "docker-compose*.yml", ".dockerignore"]
triggers: ["docker", "container", "dockerfile", "compose"]
always_apply: false
---

# Docker Production Guidelines

## Invariants
1. **Multi-Stage Builds**: Separate build-time dependencies from the minimal runtime image.
2. **Non-Root Execution**: Create and switch to a non-root user (`USER appuser`) before the entrypoint.
3. **Cache Layering**: Copy dependency manifests (`package.json`, `Cargo.toml`, `requirements.txt`) and install dependencies before copying source code.
4. **Minimal Base Images**: Use `alpine`, `distroless`, or slim Debian base images.
"#,
    },
    BuiltinSkillDef {
        name: "tdd-workflow",
        content: r#"---
name: "tdd-workflow"
description: "Use when implementing any feature or bugfix, before writing implementation code — enforces RED-GREEN-REFACTOR, the Falsifiability Gate ('name the break it catches'), mutation checks, and full suite verification via power_verify."
category: "testing"
kind: "workflow"
globs: ["tests/**/*", "*_test.*", "*.spec.*", "*.test.*"]
triggers: ["tdd", "test-driven-development", "tdd-workflow", "test", "unit test", "integration test", "writing-good-tests"]
allowed_tools: ["read_file", "write_file", "patch_file", "exec_cmd", "power_verify", "grep_search"]
always_apply: false
---

# Test-Driven Development (TDD) & Writing Good Tests (MiniPower / obra/superpowers v7.0.1)

## The Iron Law
```
NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST
```
If you wrote implementation code before the test, delete it (`git checkout -- <file>`) and start over from RED. Do not keep it as a "reference".

## The Falsifiability Gate (`writing-good-tests`)
Before writing any assertion, state in one sentence: **"This test catches `<specific break>` in `<code under test>`."**
- **Ban Mirror Assertions**: Never assert that a constant equals itself (`assert_eq!(MAX_RETRIES, 3)`) or copy the production formula into the expected side of the test. Compute expected values by hand from concrete domain examples.
- **Ban Change-Detector Tests**: Test public observable behavior (`given X -> returns Y`), never internal call order or private helper structure.
- **Ban String-Presence Checks on Executable Files**: Never test scripts, configs, or SQL by `grep`-ing or `assert!(file.contains(...))` — execute or parse them against real inputs.
- **Ban Mock-Existence Tests**: Never assert only that a mock was called; test real component integration or real output transformation.

## Red-Green-Refactor + Mutation Cycle
1. **RED — Write One Minimal Failing Test**: Use real objects and concrete expected values. Run via `exec_cmd` or `power_verify` and confirm it fails because the feature is missing (not a syntax error).
2. **GREEN — Write Minimal Code**: Write the simplest production code that makes the failing test pass.
3. **Mutation Check**: Mentally (or briefly) break the branch/operator you just wrote — would the test turn RED? If the test still passes when the logic is inverted, strengthen the assertion immediately.
4. **REFACTOR & Full Suite**: Clean up duplication, then run the **entire project test suite** via `power_verify` (not just your new test file) to prove zero regressions across the codebase.
"#,
    },
    BuiltinSkillDef {
        name: "systematic-debugging",
        content: r#"---
name: "systematic-debugging"
description: "Use when encountering any bug, test failure, flaky test, or unexpected behavior, before proposing fixes — enforces 4-phase root cause investigation, backward call-chain tracing, multi-component boundary instrumentation, 4-layer defense-in-depth, and condition-based waiting."
category: "testing"
kind: "workflow"
globs: ["**/*"]
triggers: ["bug", "error", "debug", "systematic-debugging", "crash", "fix", "failure", "root-cause", "flaky"]
allowed_tools: ["read_file", "write_file", "patch_file", "exec_cmd", "grep_search", "locate_symbol", "lsp_diagnostics", "power_verify", "vault_gotcha_add"]
always_apply: false
---

# Systematic Debugging Protocol (MiniPower / obra/superpowers v7.0.1)

## The Iron Law
```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```
Symptom fixes and "quick patches" mask structural defects. Never apply a fix until Phase 1 is complete.

## 4-Phase Protocol
1. **Phase 1: Root Cause Investigation (`root-cause-tracing`)**:
   - Capture the exact error message, stack trace, and deterministic reproduction using `exec_cmd` and `lsp_diagnostics`.
   - Use `grep_search` and `locate_symbol` to **trace backward up the call chain** from the symptom to the original caller that introduced the invalid state or bad value. Never fix solely at the leaf where the crash surfaces if the bad state originated upstream.
   - **Multi-Component Boundary Instrumentation**: When a system spans multiple layers (CLI -> orchestrator -> tool -> subprocess/DB/HTTP), log what enters and exits each boundary in a single diagnostic pass to pinpoint which boundary drops or corrupts the data before changing code.
   - **Test State Bisection (`find-polluter`)**: If a test passes in isolation but fails in a suite run, bisect the preceding test files to isolate the exact test polluting shared environment/filesystem state.
2. **Phase 2: Pattern Analysis & Hypothesis Testing**:
   - Compare against working examples in the repository. Formulate a **single falsifiable hypothesis** ("X fails because Y passes empty Z at `path:line`") and test one variable at a time.
3. **Phase 3: Minimal Fix + 4-Layer Defense-in-Depth (`defense-in-depth`)**:
   - Write a failing regression test first (`tdd-workflow`).
   - Validate at **4 layers** where applicable: (1) Entry-point input validation, (2) Business logic guard, (3) Environment/sandbox invariant check, and (4) Diagnostic `tracing::debug!` / `tracing::warn!` instrumentation.
   - **Condition-Based Waiting (`condition-based-waiting`)**: When fixing async/timing flakes, replace arbitrary `sleep()` delays with deterministic condition polling or channel notifications (`tokio::sync::Notify` / `oneshot`).
   - **3-Fix Architecture Circuit Breaker**: If 3 attempted fixes fail in the same area, STOP patching and escalate an architectural rethink via `ask_user` or `record_decision`.
4. **Phase 4: Verify & Record Gotcha**:
   - Run `power_verify` to confirm the regression test and full suite pass, then record non-obvious root causes via `vault_gotcha_add` so future agents never repeat the bug.
"#,
    },
    BuiltinSkillDef {
        name: "security-audit",
        content: r#"---
name: "security-audit"
description: "Security review guidelines: OWASP Top 10, sanitization, authentication, authorization, and secret safety."
category: "security"
kind: "workflow"
globs: ["**/*"]
triggers: ["security", "auth", "vulnerability", "token", "password", "sanitize"]
always_apply: false
---

# Security & Hardening Protocol

## Invariants
1. **Zero Secret Leaks**: Never hardcode API keys, passwords, or tokens in source code. Load from environment variables (`.env`).
2. **Input Sanitization**: Validate all external inputs against strict schemas before processing.
3. **Path Traversal Defense**: Validate file paths to prevent directory traversal (`..`) outside authorized workspace boundaries.
4. **Command Injection Prevention**: Never pass unsanitized user strings to shell command execution (`sh -c`).
"#,
    },
    BuiltinSkillDef {
        name: "gsap",
        content: r#"---
name: "gsap"
description: "GSAP animation library, timelines, scroll-driven ScrollTrigger, performance, and clean unmounting."
category: "ui-styling"
kind: "reference"
globs: ["*gsap*.*", "**/animations/**/*.ts", "**/animations/**/*.tsx"]
triggers: ["gsap", "animation", "scrolltrigger", "motion", "tween"]
always_apply: false
---

# GSAP Animation Best Practices

## Invariants
1. **Transform Performance**: Animate `x`, `y`, `scale`, and `opacity` rather than top, left, width, or height to prevent browser layout thrashing.
2. **Context Cleanup**: In React, always use `useGSAP` or wrap animations in `gsap.context()` to ensure complete cleanup on component unmount.
3. **Timeline Sequencing**: Chain related animations in a single `gsap.timeline()` with relative positioning (`"<0.1"`) rather than independent timeouts.
"#,
    },
    BuiltinSkillDef {
        name: "hono",
        content: r#"---
name: "hono"
description: "Hono modern ultrafast web framework for TypeScript and edge environments."
category: "backend"
kind: "reference"
globs: ["**/hono*.ts", "**/routes/**/*.ts"]
triggers: ["hono", "edge", "cloudflare workers", "bun"]
always_apply: false
---

# Hono Framework Standards

## Invariants
1. **Type-Safe Routing**: Use Hono RPC (`hc`) for end-to-end typed client-server communication.
2. **Standard Middleware**: Use built-in middleware (`logger`, `cors`, `secureHeaders`, `prettyJSON`).
3. **Zod Validation**: Use `@hono/zod-validator` to validate request JSON, queries, and params before handlers run.
"#,
    },
    BuiltinSkillDef {
        name: "vite",
        content: r#"---
name: "vite"
description: "Vite build tool, dev server, plugins, asset resolution, and environment variables."
category: "frontend"
kind: "reference"
globs: ["vite.config.*", "src/**/*.tsx", "src/**/*.ts"]
triggers: ["vite", "bundler", "hmr"]
always_apply: false
---

# Vite Configuration & Patterns

## Invariants
1. **Env Variables**: Access client environment variables via `import.meta.env.VITE_*`. Never access `process.env` in client code.
2. **Path Aliasing**: Configure `@/` aliases consistently in both `vite.config.ts` and `tsconfig.json`.
3. **Type Declarations**: For TypeScript projects, ensure `src/vite-env.d.ts` contains `/// <reference types="vite/client" />`.
"#,
    },
    BuiltinSkillDef {
        name: "flutter",
        content: r#"---
name: "flutter"
description: "Cross-platform mobile and web applications with Flutter, Riverpod, GoRouter, Material 3, and responsive layouts."
category: "mobile"
kind: "reference"
globs: ["*.dart", "pubspec.yaml", "pubspec.lock"]
triggers: ["flutter", "dart", "riverpod", "mobile", "cross-platform", "widget"]
always_apply: false
---

# Flutter Engineering Standards

## Invariants & Rules
1. **Declarative UI**: Build UI as functions of state. Keep widgets small and single-purpose; decompose complex build methods into separate widget classes.
2. **State Management**: Use Riverpod (NotifierProvider, AsyncNotifierProvider) for shared state. Confine setState strictly to ephemeral local widget animations.
3. **Const Constructors**: Use const constructors wherever possible to maximize widget tree caching and prevent superfluous repaints.
4. **Adaptive & Responsive Layouts**: Use LayoutBuilder, MediaQuery, and responsive breakpoints to adapt layout across mobile, tablet, and desktop screens.
5. **Clean Navigation**: Use GoRouter with typed routes and declarative redirect guards for authentication and deep linking.
"#,
    },
    BuiltinSkillDef {
        name: "mongodb",
        content: r#"---
name: "mongodb"
description: "MongoDB & Mongoose schemas, compound indexes, aggregation pipelines, and ACID transactions for Node/Bun backends."
category: "database"
kind: "reference"
globs: ["models/**/*.ts", "models/**/*.js", "mongoose*.ts", "schemas/**/*.ts"]
triggers: ["mongodb", "mongoose", "nosql", "document database", "aggregation"]
always_apply: false
---

# MongoDB & Mongoose Standards

## Invariants & Rules
1. **Schema Validation**: Define Mongoose schemas with strict types, required validations, and timestamps: true for automatic createdAt/updatedAt tracking.
2. **Explicit Indexing**: Create explicit indexes for fields frequently queried (e.g. email, status, compound lookup keys) to eliminate full collection scans.
3. **Document Modeling**: Embed subdocuments when child data is accessed together with the parent; reference collections (ObjectId) only when data grows unboundedly.
4. **Connection Singleton**: Maintain a single persistent database connection across the entire application lifecycle; never open connections per-request.
5. **ACID Transactions**: Wrap multi-document updates in sessions and transactions (session.withTransaction) when modifying dependent records.
"#,
    },
    BuiltinSkillDef {
        name: "prisma",
        content: r#"---
name: "prisma"
description: "Prisma ORM schema modeling, migrations, type-safe queries, relation mapping, and connection pool management."
category: "database"
kind: "reference"
globs: ["schema.prisma", "prisma/**/*.ts", "prisma/**/*.js", "prisma/migrations/**/*"]
triggers: ["prisma", "orm", "schema.prisma", "database migration", "prisma client"]
always_apply: false
---

# Prisma ORM Standards

## Invariants & Rules
1. **Schema Mapping**: Define relations and indexes clearly in schema.prisma. Use @map and @@map for snake_case database tables/columns while maintaining camelCase in TypeScript.
2. **Migrations**: Always generate migrations via npx prisma migrate dev (or bunx prisma migrate dev) when editing schemas. Never manipulate generated SQL migrations manually.
3. **Singleton Client**: Instantiate PrismaClient as a global singleton (globalThis) to prevent connection pool exhaustion during development hot-reloading.
4. **Referential Integrity**: Define explicit referential actions (e.g. onDelete: Cascade or onDelete: Restrict) on all foreign keys to prevent constraint violations.
5. **Selective Queries**: Use select or omit to fetch only the required fields instead of querying whole entities with heavy relations.
"#,
    },
    BuiltinSkillDef {
        name: "express",
        content: r#"---
name: "express"
description: "Express.js REST APIs with TypeScript, modular router patterns, security middleware (helmet, cors), and global error handlers."
category: "backend"
kind: "reference"
globs: ["server.ts", "server.js", "app.ts", "app.js", "routes/**/*.ts", "routes/**/*.js"]
triggers: ["express", "expressjs", "express.js", "rest api", "node server", "middleware"]
always_apply: false
---

# Express.js REST API Standards

## Invariants & Rules
1. **Security Headers**: Always mount helmet() at the top of the middleware chain to configure essential HTTP security headers.
2. **CORS Hardening**: Explicitly configure allowed origins with cors({ origin: [...] }) rather than leaving open wildcards in production.
3. **Global Error Handling**: Always mount a centralized 4-parameter error handler (err, req, res, next) at the very bottom of the middleware chain.
4. **Modular Routers**: Organize API endpoints into discrete feature routers (express.Router()) inside routes/ or controllers/; keep app.ts as a clean assembler.
5. **Async Safety**: In Express 4, wrap async route handlers in an async error wrapper (express-async-errors or try/catch) so rejected promises reach the error handler.
"#,
    },
    // =========================================================================
    // Official Anthropic Skills Catalog (anthropics/skills)
    // =========================================================================
    BuiltinSkillDef {
        name: "mcp-builder",
        content: r#"---
name: "mcp-builder"
description: "Guide for creating high-quality MCP (Model Context Protocol) servers in TypeScript (@modelcontextprotocol/sdk) or Python (FastMCP) with strict schemas, pagination, and evaluations."
category: "backend"
kind: "workflow"
globs: ["*mcp*.ts", "*mcp*.py", "**/mcp/**/*.ts", "**/mcp/**/*.py", "**/mcp/**/*.rs"]
triggers: ["mcp", "mcp-builder", "model context protocol", "fastmcp", "mcp server", "mcp tool"]
allowed_tools: ["exec_cmd", "read_file", "write_file", "patch_file"]
always_apply: false
---

# MCP Server Development Guide (anthropics/skills)

## 4-Phase MCP Engineering Workflow
1. **Deep Research & Planning**:
   - Prefer **Streamable HTTP** for remote/stateless servers and **stdio** for local process servers.
   - Design high-signal, workflow-oriented tools rather than thin 1:1 wrappers around noisy raw REST endpoints.
   - Prefix tool names consistently by service (e.g. `github_create_issue`, `jira_search_issues`) using `snake_case`.
2. **Implementation & Schema Rigor**:
   - **TypeScript (`@modelcontextprotocol/sdk`)**: Use `server.registerTool()` with strict `zod` `.strict()` input schemas, `outputSchema`, and explicit `annotations` (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`).
   - **Python (`FastMCP`)**: Use `@mcp.tool` decorators with Pydantic v2 `BaseModel` (`ConfigDict(str_strip_whitespace=True, extra='forbid')`) and `Field(description=...)`.
   - **Pagination & Truncation**: Respect `limit`/`offset` or `cursor` pagination, cap responses to prevent context bloat, and support both `"markdown"` and `"json"` `response_format` options.
   - **Actionable Errors**: Return clear, next-step-guiding error messages (`isError: true`) rather than raw stack traces.
3. **Review & Verification**:
   - Never write logs to `stdout` in `stdio` MCP servers (it corrupts JSON-RPC framing); log exclusively to `stderr` or file.
   - Compile and type-check (`npm run build` / `npx tsc --noEmit` or `python -m py_compile`) before testing.
4. **Evaluation Harness**:
   - Verify tool discoverability and multi-step tool composition with realistic read-only evaluation queries.
"#,
    },
    BuiltinSkillDef {
        name: "webapp-testing",
        content: r#"---
name: "webapp-testing"
description: "Toolkit for interacting with and testing local web applications using Playwright and headless browser automation (reconnaissance-then-action workflow, DOM inspection, console/network verification)."
category: "testing"
kind: "workflow"
globs: ["playwright.config.*", "e2e/**/*", "tests/**/*.spec.ts", "tests/**/*.test.ts"]
triggers: ["webapp-testing", "playwright", "e2e", "browser test", "ui test", "headless browser", "verify webapp"]
allowed_tools: ["exec_cmd", "browser_navigate", "browser_screenshot", "browser_eval", "browser_click"]
always_apply: false
---

# Web Application Testing Protocol (anthropics/skills)

## Reconnaissance-Then-Action Pattern
1. **Server Lifecycle First**:
   - Check if the dev server is already running before launching a new one. If starting a background dev server, wait until the port is listening before navigating.
2. **Navigate & Wait for Hydration**:
   - Always wait for `networkidle` (`page.wait_for_load_state('networkidle')`) on dynamic SPAs before inspecting the DOM or taking screenshots.
3. **Inspect Rendered DOM Before Selecting**:
   - Never guess brittle CSS selectors from source code alone. Capture a screenshot, inspect rendered buttons/inputs/links (`page.locator('button, a, input, [role="button"]')`), and use semantic role/text/label locators.
4. **Zero Console/Page Errors**:
   - Attach listeners to `page.on('console')` and `page.on('pageerror')` during E2E runs and fail verification if uncaught runtime exceptions or failed API requests occur.
"#,
    },
    BuiltinSkillDef {
        name: "skill-creator",
        content: r#"---
name: "skill-creator"
description: "Create new Agent Skills, modify and improve existing SKILL.md files, design 3-level progressive disclosure skill directories (scripts/, references/, assets/), and optimize trigger descriptions."
category: "workflow"
kind: "workflow"
globs: ["**/SKILL.md", ".minicode/skills/**/*"]
triggers: ["skill-creator", "create skill", "new skill", "author skill", "skill.md", "progressive disclosure"]
allowed_tools: ["vault_create", "vault_update", "vault_show", "vault_search", "write_file", "patch_file"]
always_apply: false
---

# Agent Skill Architecture & Authoring Guide (anthropics/skills)

## 3-Level Progressive Disclosure System
1. **Level 1 — YAML Frontmatter Metadata (~100 words, always in context)**:
   - `name` (kebab-case identifier) and `description` (both *what* the skill does and specific contexts/keywords for *when* to trigger it).
   - Include `triggers`, `globs`, `category`, `kind` (`workflow` or `reference`), and `allowed_tools`.
   - Make `description` and `triggers` slightly "pushy" so the orchestrator reliably activates the skill even when the user does not name it explicitly.
2. **Level 2 — `SKILL.md` Body (<500 lines, loaded when triggered)**:
   - Imperative, high-signal instructions, decision trees, and explain-the-*why* invariants rather than rigid boilerplate.
   - Keep `SKILL.md` under 500 lines; offload large API tables or domain schemas to Level 3.
3. **Level 3 — Bundled Resources (`scripts/`, `references/`, `assets/`)**:
   - `scripts/`: Deterministic helper scripts for repetitive/fragile tasks.
   - `references/`: Detailed domain guides read on-demand when needed.
   - `assets/`: Templates, fonts, or boilerplate copied/used in output.
   - Clearly link each bundled resource from `SKILL.md` with explicit guidance on *when* to read or execute it.
"#,
    },
    BuiltinSkillDef {
        name: "doc-coauthoring",
        content: r#"---
name: "doc-coauthoring"
description: "Structured 3-stage workflow for co-authoring technical specs, PRDs, RFCs, ADRs, and decision documents: Context Gathering, Section-by-Section Refinement, and Reader Verification."
category: "workflow"
kind: "workflow"
globs: ["docs/**/*.md", "*.prd.md", "RFC*.md", "ADR*.md", "SPEC*.md"]
triggers: ["doc-coauthoring", "write a doc", "draft a spec", "rfc", "adr", "design doc", "technical spec", "proposal"]
allowed_tools: ["read_file", "write_file", "patch_file"]
always_apply: false
---

# Structured Document Co-Authoring Workflow (anthropics/skills)

## Stage 1: Context Gathering
1. Clarify **Document Type**, **Primary Audience**, **Desired Impact** (what decision or action should the reader take after reading), and any **Template/Format** constraints.
2. Ingest existing workspace context (`README.md`, architecture docs, related code modules) before asking the user questions.
3. Ask 5–10 focused, high-leverage gap-filling questions to eliminate ambiguity before drafting prose.

## Stage 2: Refinement & Structure (Section-by-Section)
1. Scaffold the document outline with clear section headers first.
2. For each section:
   - Present 3–5 brief brainstorming/direction options if trade-offs exist.
   - Draft the section concisely using tables, Mermaid diagrams, and concrete code/API contracts rather than filler prose.
   - Apply surgical edits (`patch_file`) based on user feedback.

## Stage 3: Reader Testing & Verification
1. Audit the completed document from the perspective of a cold reader with zero prior context:
   - Are assumptions, non-goals, failure modes, and concrete examples explicit?
   - Are there any internal contradictions, undefined acronyms, or ambiguous pronouns?
"#,
    },
    BuiltinSkillDef {
        name: "web-artifacts-builder",
        content: r#"---
name: "web-artifacts-builder"
description: "Suite of patterns for creating elaborate, multi-component interactive web artifacts and single-file HTML bundles using React 18/19, TypeScript, Tailwind CSS, and shadcn/ui."
category: "frontend"
kind: "workflow"
globs: ["*.artifact.html", "*.bundle.html", "artifacts/**/*.tsx", "artifacts/**/*.html"]
triggers: ["web-artifacts-builder", "web artifact", "html artifact", "interactive artifact", "single-file html", "shadcn artifact"]
allowed_tools: ["exec_cmd", "write_file", "patch_file", "browser_navigate", "browser_screenshot"]
always_apply: false
---

# Web Artifacts Builder Protocol (anthropics/skills)

## Architecture & Anti-Slop Design Rules
1. **Avoid AI Slop**: Never use generic centered purple-gradient hero cards with rounded-xl 3-column icon grids. Commit to an intentional editorial, Swiss grid, neo-brutalist, or high-craft dark UI aesthetic.
2. **Component Modularity**:
   - Build rich state machines, interactive filters, charts, and keyboard-accessible dialogs using React + TypeScript + Tailwind CSS + Lucide icons.
3. **Single-File Portable Bundling (when requested)**:
   - Bundle multi-component React/Tailwind apps into a self-contained `.html` file using Vite + `vite-plugin-singlefile` or Parcel/esbuild inline bundling so the artifact opens cleanly in any browser with zero external server required.
4. **Visual Verification**:
   - Always render and inspect the generated artifact in the headless browser (`browser_navigate` + `browser_screenshot`) to verify layout alignment, contrast, and zero runtime console errors.
"#,
    },
    BuiltinSkillDef {
        name: "canvas-design",
        content: r#"---
name: "canvas-design"
description: "Create museum-grade visual art, posters, and spatial graphic designs in PNG, PDF, or SVG using a two-step Visual Philosophy Manifesto + high-craft spatial composition."
category: "frontend"
kind: "workflow"
globs: ["*.pdf", "posters/**/*", "canvas/**/*"]
triggers: ["canvas-design", "poster design", "visual art", "svg poster", "graphic design", "print pdf"]
allowed_tools: ["exec_cmd", "write_file", "patch_file"]
always_apply: false
---

# Canvas Design & Spatial Composition Skill (anthropics/skills)

## Two-Phase Creation Process
1. **Phase 1 — Visual Philosophy Manifesto**:
   - Define a cohesive aesthetic movement (e.g., *Concrete Swiss Grid*, *Chromatic Silence*, *Analog Instrument Telemetry*) grounded in **space, form, color, scale, and typographic hierarchy**.
2. **Phase 2 — Museum-Grade Canvas Execution**:
   - Use programmatic composition (ReportLab/Cairo/SVG/HTML5 Canvas) with mathematical grid alignment, generous negative space, and deliberate focal points.
   - **Zero Overlaps & Zero Clipping**: Every text label, geometric mark, and margin must respect strict bounding-box clearance.
   - **Typography as Visual Element**: Pair bold geometric display numerals/titles with whisper-quiet micro-captions and hairline rules.
"#,
    },
    BuiltinSkillDef {
        name: "algorithmic-art",
        content: r#"---
name: "algorithmic-art"
description: "Creating algorithmic and generative art using p5.js or HTML5 Canvas with seeded randomness, flow fields, particle systems, noise fields, and interactive parameter controls."
category: "frontend"
kind: "workflow"
globs: ["*sketch*.js", "*sketch*.html", "*p5*.html", "*generative*.html"]
triggers: ["algorithmic-art", "generative art", "p5.js", "p5js", "flow field", "perlin noise", "particle system", "creative coding"]
allowed_tools: ["write_file", "patch_file", "browser_navigate", "browser_screenshot"]
always_apply: false
---

# Algorithmic & Generative Art Skill (anthropics/skills)

## Generative Art Architecture
1. **Computational Aesthetic Philosophy**:
   - Express beauty through emergent mathematical process — Perlin/Simplex noise fields, curl flow fields, reaction-diffusion, wave interference, or recursive spatial subdivision — rather than static pre-drawn shapes.
2. **Seeded Reproducibility (`randomSeed` & `noiseSeed`)**:
   - Always drive every stochastic decision through an explicit integer seed (`randomSeed(seed)`, `noiseSeed(seed)`) with UI controls for Prev / Next / Random Seed and Jump-to-Seed.
3. **Interactive Parameter Sidebar**:
   - Expose 4–8 meaningful mathematical parameters (particle count, noise scale, turbulence octaves, damping, color palette) with live sliders and a "Download PNG" action.
"#,
    },
    BuiltinSkillDef {
        name: "theme-factory",
        content: r#"---
name: "theme-factory"
description: "Curated 10-theme color palette and typography pairing system (Ocean Depths, Sunset Boulevard, Forest Canopy, Modern Minimalist, Midnight Galaxy, Botanical Garden, etc.) plus custom theme synthesis."
category: "ui-styling"
kind: "reference"
globs: ["**/theme*.*", "**/*.css", "**/*.pptx", "**/*.html"]
triggers: ["theme-factory", "color palette", "font pairing", "theme preset", "retheme", "brand palette"]
always_apply: false
---

# Theme Factory — Curated Palettes & Typography (anthropics/skills)

## Curated Theme Presets
1. **Ocean Depths**: Deep Navy `#1a2332`, Teal `#2d8b8b`, Seafoam `#a8dadc`, Cream `#f1faee` — *DejaVu Sans Bold + DejaVu Sans*
2. **Sunset Boulevard**: Burnt Orange `#e76f51`, Coral `#f4a261`, Warm Sand `#e9c46a`, Deep Purple `#264653` — *DejaVu Serif Bold + DejaVu Sans*
3. **Forest Canopy**: Forest Green `#2d4a2b`, Sage `#7d8471`, Olive `#a4ac86`, Ivory `#faf9f6` — *FreeSerif Bold + FreeSans*
4. **Modern Minimalist**: Charcoal `#36454f`, Slate `#708090`, Silver `#d3d3d3`, White `#ffffff` — *DejaVu Sans Bold + DejaVu Sans*
5. **Golden Hour**: Mustard `#f4a900`, Terracotta `#c1666b`, Warm Beige `#d4b896`, Chocolate `#4a403a` — *FreeSans Bold + FreeSans*
6. **Arctic Frost**: Ice Blue `#d4e4f7`, Steel Blue `#4a6fa5`, Silver `#c0c0c0`, Crisp White `#fafafa` — *DejaVu Sans Bold + DejaVu Sans*
7. **Desert Rose**: Dusty Rose `#d4a5a5`, Clay `#b87d6d`, Sand `#e8d5c4`, Deep Burgundy `#5d2e46` — *FreeSerif Bold + FreeSans*
8. **Tech Innovation**: Electric Blue `#0066ff`, Neon Cyan `#00ffff`, Dark Charcoal `#1e1e1e`, White `#ffffff` — *DejaVu Sans Bold + DejaVu Sans Mono*
9. **Botanical Garden**: Fern `#4a7c59`, Marigold `#f9a620`, Terracotta `#b7472a`, Cream `#f5f3ed` — *DejaVu Serif Bold + DejaVu Sans*
10. **Midnight Galaxy**: Deep Purple `#2b1e3e`, Cosmic Blue `#4a4e8f`, Lavender `#a490c2`, Silver `#e6e6fa` — *FreeSans Bold + FreeSans*

## Application Rules
- Maintain WCAG AA contrast ratios between foreground text and surface backgrounds.
- Apply primary accent sparingly (CTAs, active indicators, key metrics) and use neutral surfaces for 80%+ of layout area.
"#,
    },
    BuiltinSkillDef {
        name: "slack-gif-creator",
        content: r#"---
name: "slack-gif-creator"
description: "Create animated GIFs optimized for Slack (128x128 emoji GIFs and 480x480 message GIFs) using Python PIL/Pillow with easing curves, particle bursts, and palette compression."
category: "frontend"
kind: "workflow"
globs: ["*.gif", "*gif*.py"]
triggers: ["slack-gif-creator", "animated gif", "slack gif", "emoji gif", "make a gif"]
allowed_tools: ["exec_cmd", "write_file"]
always_apply: false
---

# Slack Animated GIF Creator Skill (anthropics/skills)

## Dimensions & Frame Budget
- **Emoji GIF**: `128x128` at 10–15 FPS (12–30 frames) with bold shapes, thick outlines (`width >= 2`), and zero tiny text.
- **Message GIF**: `480x480` at 15–20 FPS (20–45 frames).

## Animation Polish Invariants
1. **Never Use Linear Motion**: Always apply easing curves (`ease_out_cubic`, `ease_out_back`, `ease_out_elastic`, `bounce_out`) plus squash-and-stretch on impacts.
2. **Seamless Looping**: Ensure frame `N-1` transitions smoothly back to frame `0` via phase-locked sine/cosine cycles (`2 * math.pi * i / num_frames`).
3. **File Size Optimization**: Quantize frames with `Image.ADAPTIVE` palette (64–128 colors) and save with `optimize=True, loop=0`.
"#,
    },
    BuiltinSkillDef {
        name: "claude-api",
        content: r#"---
name: "claude-api"
description: "Build LLM applications with the Anthropic Claude API and SDKs (TypeScript @anthropic-ai/sdk, Python anthropic): Messages API, streaming, adaptive thinking, tool use, and prompt caching."
category: "backend"
kind: "reference"
globs: ["*anthropic*.ts", "*anthropic*.py", "*claude*.ts", "*claude*.py"]
triggers: ["claude-api", "anthropic sdk", "@anthropic-ai/sdk", "messages.create", "messages.stream", "claude tool use", "prompt caching"]
always_apply: false
---

# Claude API & Anthropic SDK Guide (anthropics/skills)

## Core API Patterns & Invariants
1. **Current Models**: Default to `claude-sonnet-4-6` (high-speed coding/agents), `claude-opus-4-6` (deep reasoning), or `claude-haiku-4-5` (low-latency classification).
2. **Adaptive Thinking**: For Claude 4.6 models, prefer `thinking: { type: "adaptive" }` with `output_config: { effort: "medium" | "high" }` rather than deprecated manual `budget_tokens`.
3. **Streaming Required for Large Outputs**: Always use `.stream()` (`client.messages.stream(...)`) with `.finalMessage()` / `.get_final_message()` when `max_tokens > 4096` to avoid HTTP timeouts.
4. **Structured Outputs & Tool Use**:
   - Define strict JSON schemas (`additionalProperties: false`) for tools or `output_config.format`.
   - When handling `stop_reason == "tool_use"`, execute all requested `tool_use` blocks and return all `tool_result` blocks in a single subsequent `user` message.
5. **Prompt Caching**: Add `cache_control: { type: "ephemeral" }` on large static system prompts or tool definitions to cut input token costs by up to 90% and latency by up to 80%.
"#,
    },
    BuiltinSkillDef {
        name: "docx",
        content: r#"---
name: "docx"
description: "Create, read, edit, and redline Microsoft Word (.docx) documents using pandoc, docx-js (JavaScript/TypeScript), or Python OOXML / python-docx with tracked changes (<w:ins>, <w:del>)."
category: "docs"
kind: "workflow"
globs: ["*.docx", "*docx*.js", "*docx*.ts", "*docx*.py"]
triggers: ["docx", "word document", ".docx", "redline", "tracked changes", "ooxml", "python-docx", "docx-js"]
allowed_tools: ["exec_cmd", "read_file", "write_file", "patch_file"]
always_apply: false
---

# Word Document (.docx) Creation, Editing & Redlining (anthropics/skills)

## Workflow Decision Tree
1. **Reading / Extracting Text**:
   - Run `pandoc --track-changes=all input.docx -o output.md` or use `python-docx` (`docx.Document("input.docx")`) to inspect paragraphs, tables, and styles.
2. **Creating a New `.docx` Document**:
   - Use **`docx` (`docx-js`)** in Node/TypeScript (`Document`, `Packer.toBuffer`, `Paragraph`, `TextRun`, `Table`, `HeadingLevel`) or **`python-docx`** in Python.
   - Establish consistent typography (1-inch margins, clean heading hierarchy, explicit table column widths, and subtle header shading).
3. **Editing / Redlining an Existing `.docx` Document**:
   - A `.docx` file is a ZIP archive containing `word/document.xml`, `word/comments.xml`, and `word/styles.xml`.
   - For tracked changes (redlining), only wrap the minimal changed span in `<w:del><w:r><w:delText>old</w:delText></w:r></w:del><w:ins><w:r><w:t>new</w:t></w:r></w:ins>` while preserving the original `<w:r>` runs for surrounding unchanged text.
"#,
    },
    BuiltinSkillDef {
        name: "pdf",
        content: r#"---
name: "pdf"
description: "Extract text and tables from PDFs, merge/split/rotate pages, fill PDF forms, and generate polished PDF reports using pypdf, pdfplumber, reportlab, and poppler/qpdf."
category: "docs"
kind: "workflow"
globs: ["*.pdf", "*pdf*.py"]
triggers: ["pdf", ".pdf", "pypdf", "pdfplumber", "reportlab", "extract pdf", "merge pdf", "pdf form"]
allowed_tools: ["exec_cmd", "read_file", "write_file", "patch_file"]
always_apply: false
---

# PDF Processing & Generation Guide (anthropics/skills)

## Tool Selection Matrix
1. **Read & Extract Text / Tables**:
   - Use **`pdfplumber`** (`pdfplumber.open("doc.pdf")`, `page.extract_text()`, `page.extract_tables()`) for layout-preserving text and table extraction into `pandas`.
   - Use `pdftotext -layout input.pdf output.txt` or `pdftoppm -png -r 150 input.pdf page` for fast CLI extraction or visual page rendering.
2. **Merge, Split, Rotate & Metadata**:
   - Use **`pypdf`** (`PdfReader`, `PdfWriter`) or `qpdf` to merge documents, split pages, rotate orientations, or encrypt/decrypt PDFs.
3. **Generate New PDFs**:
   - Use **`reportlab`** (`SimpleDocTemplate`, `Paragraph`, `Table`, `TableStyle`, `Spacer`, `PageBreak`) for multi-page data reports with automatic pagination and repeating table headers (`repeatRows=1`).
4. **Visual Inspection**:
   - After generating or filling a PDF, render pages to PNG (`pdftoppm -png -r 150`) to verify zero text overlapping or clipped tables.
"#,
    },
    BuiltinSkillDef {
        name: "pptx",
        content: r#"---
name: "pptx"
description: "Create, analyze, and edit PowerPoint (.pptx) slide decks using python-pptx, markitdown, or OOXML unpacking with strong visual hierarchy, color palettes, and slide layouts."
category: "docs"
kind: "workflow"
globs: ["*.pptx", "*pptx*.py", "*slides*.py"]
triggers: ["pptx", "powerpoint", ".pptx", "slide deck", "presentation", "python-pptx", "slides"]
allowed_tools: ["exec_cmd", "read_file", "write_file", "patch_file"]
always_apply: false
---

# PowerPoint (.pptx) Presentation Engineering (anthropics/skills)

## Reading & Analyzing Presentations
- Extract slide text and notes via `python -m markitdown deck.pptx` or inspect `ppt/slides/slide*.xml` and `ppt/theme/theme1.xml` by unzipping the `.pptx` archive.

## Creating & Designing Slide Decks
1. **Content-Informed Visual System**:
   - Select a cohesive 3–5 color palette (dominant surface, high-contrast foreground text, muted secondary tone, and 1 bold accent color) before writing code.
   - Use 16:9 widescreen (`prs.slide_width = Inches(13.333)`, `prs.slide_height = Inches(7.5)`).
2. **Slide Layout & Anti-Crowding Invariants**:
   - **One Core Idea Per Slide**: Never cram more than 4–5 bullet points or 2 side-by-side cards onto a single slide.
   - **Strong Typographic Contrast**: 36–48pt bold titles, 20–24pt section callouts, and 14–16pt body copy using web-safe fonts (Arial, Helvetica, Georgia, Trebuchet MS, Courier New).
   - **Explicit Bounding Boxes**: Verify that title boxes, metric cards, charts, and footer captions have zero overlapping coordinates.
"#,
    },
    BuiltinSkillDef {
        name: "xlsx",
        content: r#"---
name: "xlsx"
description: "Create, analyze, and format Excel (.xlsx) spreadsheets and financial models using openpyxl and pandas with dynamic Excel formulas, zero formula errors, and industry color coding."
category: "docs"
kind: "workflow"
globs: ["*.xlsx", "*.xlsm", "*excel*.py", "*xlsx*.py"]
triggers: ["xlsx", "excel", ".xlsx", "spreadsheet", "openpyxl", "financial model", "workbook"]
allowed_tools: ["exec_cmd", "read_file", "write_file", "patch_file"]
always_apply: false
---

# Excel (.xlsx) Spreadsheet & Financial Modeling Guide (anthropics/skills)

## Non-Negotiable Spreadsheet Invariants
1. **Use Dynamic Excel Formulas — Never Hardcode Calculated Outputs**:
   - Always write live Excel formulas (`=SUM(B2:B9)`, `=INDEX(MATCH(...))`, `=(C5-B5)/B5`) into calculated cells via `openpyxl` rather than computing totals in Python and writing static numbers.
2. **Zero Formula Errors**:
   - Every delivered workbook must have **zero** `#REF!`, `#DIV/0!`, `#VALUE!`, `#N/A`, or `#NAME?` errors. Wrap division in `=IFERROR(..., 0)` where appropriate.
3. **Financial Modeling Color & Number Standards**:
   - **Blue text (`#0000FF`)**: Hardcoded assumptions and scenario inputs.
   - **Black text (`#000000`)**: Same-sheet formulas and calculations.
   - **Green text (`#008000`)**: Cross-sheet formula links (`=Assumptions!B4`).
   - Format currencies as `$#,##0;($#,##0);"-"`, percentages as `0.0%`, multiples as `0.0x`, and years as text strings (`"2026"`).
   - Auto-fit column widths (`ws.column_dimensions[col].width = max_len + 5`) so values never render as `###`.
"#,
    },
    BuiltinSkillDef {
        name: "brand-guidelines",
        content: r#"---
name: "brand-guidelines"
description: "Anthropic official brand identity, color tokens (#141413 dark, #faf9f5 light, #d97757 terracotta orange, #6a9bcc blue, #788c5d green), and Poppins/Lora typography system."
category: "ui-styling"
kind: "reference"
globs: ["**/brand*.*", "**/*.css", "**/*.pptx", "**/*.html"]
triggers: ["brand-guidelines", "anthropic brand", "brand styling", "corporate identity", "brand colors"]
always_apply: false
---

# Anthropic Brand Identity & Styling Guidelines (anthropics/skills)

## Official Color Tokens
- **Primary Dark**: `#141413` — Primary text and dark surfaces
- **Primary Light**: `#FAF9F5` — Warm editorial parchment background and text on dark
- **Mid Gray**: `#B0AEA5` — Secondary labels, borders, and muted metadata
- **Light Gray**: `#E8E6DC` — Subtle card surfaces and dividers
- **Primary Accent (Terracotta Orange)**: `#D97757` — Primary CTAs, highlights, and key data series
- **Secondary Accent (Slate Blue)**: `#6A9BCC` — Secondary charts and informational badges
- **Tertiary Accent (Sage Green)**: `#788C5D` — Positive indicators and tertiary accents

## Typography Hierarchy
- **Headings (24pt+)**: `Poppins` (semi-bold/bold) with `Arial` / `system-ui` fallback
- **Editorial Body Copy**: `Lora` (serif) with `Georgia` fallback
- **Code & Data Telemetry**: `JetBrains Mono` / `Fira Code` / `monospace`
"#,
    },
    BuiltinSkillDef {
        name: "internal-comms",
        content: r#"---
name: "internal-comms"
description: "Write high-signal internal engineering communications: 3P updates (Progress, Plans, Problems), incident reports, leadership briefs, company newsletters, and project status updates."
category: "docs"
kind: "workflow"
globs: ["comms/**/*.md", "updates/**/*.md", "postmortem*.md"]
triggers: ["internal-comms", "3p update", "status report", "incident report", "leadership update", "company newsletter", "weekly update"]
allowed_tools: ["read_file", "write_file", "patch_file"]
always_apply: false
---

# Internal Engineering Communications Guide (anthropics/skills)

## Communication Formats & Structure
1. **3P Team Update (Progress, Plans, Problems)**:
   - **TL;DR (1–2 sentences)**: Overall health status (`🟢 On Track` / `🟡 At Risk` / `🔴 Blocked`) and headline outcome.
   - **Progress**: Concrete shipped milestones with metrics or PR/issue links (outcomes, not activity logs).
   - **Plans**: Top 3–5 priorities for the next period with clear owners and target dates.
   - **Problems / Risks**: Active blockers, cross-team dependencies, and specific asks/decisions needed from leadership.
2. **Incident Report / Postmortem**:
   - **Executive Summary**: Impact window, affected users/services, root cause in plain language, and current status.
   - **Timeline (UTC)**: Timestamped detection, escalation, mitigation, and resolution milestones.
   - **Root Cause & Contributing Factors**: Technical 5-Whys analysis without blame.
   - **Action Items**: Concrete preventative engineering tasks with priority (`P0`/`P1`) and owners.
3. **Leadership Brief / FAQ**:
   - Lead with the bottom-line recommendation or answer in the first sentence, followed by 3 supporting data points.
"#,
    },
    BuiltinSkillDef {
        name: "using-minipower",
        content: r#"---
name: "using-minipower"
description: "Use when starting any conversation or task — establishes how to discover and load MiniVault/MiniKit skills via vault_load before taking action, skill priority order (process skills before domain skills), and anti-rationalization red flags."
category: "workflow"
kind: "workflow"
globs: []
triggers: ["using-minipower", "using-superpowers", "superpowers", "minipower", "skill-routing", "red-flags"]
allowed_tools: ["vault_load", "vault_search", "kit_skill_list", "power_brainstorm", "power_plan", "power_review", "power_verify", "power_worktree_task", "power_diagnose"]
always_apply: true
---

# Using MiniPower & Skill Discipline (MiniPower / obra/superpowers v7.0.1)

<SUBAGENT-STOP>
If you were dispatched as a worker subagent to execute a specific task brief (`power_worktree_task` or `delegate_task`), skip orchestrator skill routing and execute your assigned task brief directly.
</SUBAGENT-STOP>

## The Rule
Invoke relevant skills (`vault_load`) **BEFORE** any response, exploration, or code edit. Even a 1% chance a skill applies means you should check `vault_search` or `vault_load`.

## Skill Priority Order
When multiple skills apply, load and follow them in this exact order:
1. **Process skills first** (`brainstorming`, `systematic-debugging`, `writing-plans`, `executing-plans`, `subagent-driven-development`, `tdd-workflow`, `verification-before-completion`) — these govern HOW to approach the task.
2. **Implementation & domain skills second** (`rust-tokio`, `react`, `nextjs`, `frontend-design`, `mcp-builder`, etc.) — these govern domain-specific execution details.

## MiniPower Red Flags (STOP Immediately If You Think Any of These)
| Thought | Reality |
|---|---|
| "This is just a simple question / one-line change" | Check for applicable skills before acting. |
| "I need more context before checking skills" | Skill check comes BEFORE undirected exploration. |
| "I remember what `power_plan` or `tdd-workflow` says" | Skills and prompts evolve — call `power_brainstorm`, `power_plan`, `power_verify`, or `vault_load`. |
| "Tests probably pass, I'll claim completion" | Never claim completion without a fresh `power_verify` pass in `.minicode/plan/last_verify.json`. |
"#,
    },
    BuiltinSkillDef {
        name: "brainstorming",
        content: r#"---
name: "brainstorming"
description: "You MUST use this before any creative work — creating features, building components, adding functionality, or modifying behavior. Explores codebase context, ceremony sizing, visual companion mockups, user intent via ask_user, and design specs before implementation."
category: "workflow"
kind: "workflow"
globs: ["minikit_docs/core/spec.md", "minikit_docs/core/prd.md", "docs/**/*.md"]
triggers: ["brainstorming", "brainstorm", "power_brainstorm", "design", "spec", "new feature", "architecture"]
allowed_tools: ["power_brainstorm", "read_file", "grep_search", "locate_symbol", "ask_user", "record_decision", "write_file", "patch_file", "delegate_task", "browser_navigate", "browser_screenshot"]
always_apply: false
---

# Brainstorming Ideas Into Designs (MiniPower / obra/superpowers v7.0.1)

<HARD-GATE>
Do NOT invoke any implementation skill, write any production code, or scaffold any project until you have explored the codebase, presented the design, and obtained approval (or recorded autonomous `[D1]..[DN]` rulings when the user asks you to run autonomously).
</HARD-GATE>

## Checklist & Flow (`power_brainstorm`)
1. **Ground First**: Inspect relevant files, symbols, and recent commits via `read_file`, `grep_search`, and `locate_symbol` before asking any questions.
2. **Play It Back**: State your current understanding of the codebase and goal in a few sentences to confirm alignment.
3. **Proportionate Ceremony (`CeremonySize`)**: Call `power_brainstorm` with `size`:
   - `quick_task`: Fast confirm of intent -> implement directly with `tdd-workflow` & `power_verify`.
   - `spike`: Time-boxed throwaway exploration to answer one technical unknown before spec writing.
   - `small_change`: Brief inline design summary -> `power_plan`.
   - `project`: Full spec in `<docs_dir>/core/spec.md` + cold-reader Builder Check (`power_brainstorm` with `run_builder_check=true`).
4. **Clarify & Propose Options (`ask_user` + Visual Companion)**:
   - Never guess or assume when you need clarity, domain data, or lack confidence—use `ask_user` with as many questions as needed and 4 to 6 distinct options per choice question (recommended option first with `recommended: true` and a 1-line rationale).
   - **Visual Companion (`visual_mockup`)**: For UI/layout choices where seeing beats describing, pass an HTML mockup snippet in `power_brainstorm(visual_mockup=...)` (saved to `.minicode/brainstorm/<slug>.html`) and inspect/preview via `browser_navigate` / `browser_screenshot`.
5. **Write Design Spec & Record Decisions**: Write the approved design to `<docs_dir>/core/spec.md` (`power_brainstorm(spec=...)`) and log core architectural invariants to `.minicode/decisions.md` via `record_decision`.
6. **Cold-Reader Builder Check (`Project` size)**: Run `power_brainstorm(topic, run_builder_check=true)` (or dispatch a read-only `delegate_task`) to check where an implementer would have to guess on paths, types, or edge cases.
7. **Transition**: Invoke `power_plan` (`writing-plans` skill) to create the implementation plan.
"#,
    },
    BuiltinSkillDef {
        name: "writing-plans",
        content: r#"---
name: "writing-plans"
description: "Use when you have a spec or requirements for a multi-step task, before touching code — generates bite-sized TDD plans in minikit_docs/core/todo.md and .minicode/plan/task_plan.md with Interfaces (Consumes/Produces), 5-item Review Focus, SDD task briefs, and self-review."
category: "workflow"
kind: "workflow"
globs: ["minikit_docs/core/todo.md", ".minicode/plan/task_plan.md", "docs/plans/**/*.md"]
triggers: ["writing-plans", "power_plan", "create_plan", "implementation plan", "task breakdown", "roadmap"]
allowed_tools: ["power_plan", "create_plan", "read_file", "grep_search", "locate_symbol", "record_decision", "write_file", "patch_file"]
always_apply: false
---

# Writing Implementation Plans (MiniPower / obra/superpowers v7.0.1)

## Core Principles (`power_plan` & `create_plan`)
1. **Pre-Planning Conflict Check**: Before decomposing tasks, check the spec against the live codebase (`read_file`, `grep_search`, `locate_symbol`). Surface any naming, type, or architectural conflicts immediately.
2. **Decision-Focused, Bite-Sized TDD Tasks**: Persist the plan to `<docs_dir>/core/todo.md` and `.minicode/plan/task_plan.md` via `power_plan` and `create_plan` (which also initializes `.minicode/sdd/<slug>/progress.md` and `task-<id>-brief.md` files). Each task is one coherent unit of behavior (RED failing test -> run `power_verify` -> minimal GREEN code -> run `power_verify` -> commit).
3. **Explicit Cross-Task Interfaces (`Consumes` / `Produces`)**:
   - Every task must list exact file paths (`Create` / `Modify` / `Test`) and its `Interfaces: Consumes / Produces` (exact function signatures, struct/enum types, or CLI flags shared across tasks).
   - Never leave `TODO`, `TBD`, or "similar to Task N" placeholders — each task brief must be self-contained for an isolated worker.
4. **5-Item Review Focus**: Populate `review_focus` in `power_plan` with up to 5 implied inputs or failure modes that the spec implies and task tests should pin.
5. **5-Point Self-Review**:
   - **Spec coverage**: Every requirement maps to a task.
   - **Placeholder scan**: Zero `TODO`/`TBD` or vague steps.
   - **Interface consistency**: `Produces` in Task N matches `Consumes` in Task N+1.
   - **Ordering**: Earlier tasks never depend on later tasks.
   - **Proportion check**: Combine trivial back-to-back edits in the same file into a single task.
6. **Execution Handoff**: Execute automatically via (1) **Subagent-Driven Execution** (`subagent-driven-development` via `power_worktree_task`) or (2) **Native Inline Execution** (`executing-plans` via `update_progress` + `power_verify`).
"#,
    },
    BuiltinSkillDef {
        name: "executing-plans",
        content: r#"---
name: "executing-plans"
description: "Use when you have a written implementation plan in .minicode/plan/task_plan.md or minikit_docs/core/todo.md to execute inline or via reactive DAG waves with TDD verification gates, SDD progress ledger updates, and 'Rulings, not stalls' decision logging."
category: "workflow"
kind: "workflow"
globs: [".minicode/plan/task_plan.md", "minikit_docs/core/todo.md", ".minicode/decisions.md"]
triggers: ["executing-plans", "execute plan", "update_progress", "inline execution", "task_plan", "execute_task_dag"]
allowed_tools: ["read_file", "write_file", "patch_file", "exec_cmd", "update_progress", "record_decision", "power_verify", "power_review", "archive_plan", "execute_task_dag"]
always_apply: false
---

# Executing Plans Inline & Reactive DAG Waves (MiniPower / obra/superpowers v7.0.1)

## Process
1. **Load & Review Plan Critically**: Read `.minicode/plan/task_plan.md`, `.minicode/sdd/*/progress.md`, and `<docs_dir>/core/todo.md`. Verify referenced files and `Consumes`/`Produces` interfaces against the codebase before starting Task 1.
2. **Choose Inline vs Reactive DAG Wave Execution**:
   - **Inline Execution**: For tightly coupled sequential steps in the same file, execute inline (`update_progress` -> RED failing test -> `power_verify` -> minimal GREEN code -> `power_verify` -> `update_progress(status="completed")`).
   - **Reactive DAG Execution (`execute_task_dag`)**: When the plan contains parallelizable tasks with explicit upstream `depends_on` edges and disjoint file ownership, dispatch the task graph via `execute_task_dag(tasks=[...])` so ready wave-0 tasks run concurrently and downstream tasks auto-spawn the millisecond their dependencies complete.
3. **Rulings, Not Stalls**:
   - When a minor ambiguity arises where one option is clearly consistent with the spec and codebase, **make the ruling**, record it via `record_decision` (`[D1]..[DN]` in `.minicode/decisions.md`) with rationale, and keep moving.
   - Stop and ask (`ask_user`) **only** when a test/dependency is genuinely broken or two mutually exclusive product designs are equally valid.
4. **Completion Verification & Review**:
   - Run the full project verification suite via `power_verify` (writes `.minicode/plan/last_verify.json` and updates `.minicode/sdd/<slug>/progress.md`), run `power_review`, present all `[D1]..[DN]` rulings made during execution, and transition to `finishing-a-development-branch`.
"#,
    },
    BuiltinSkillDef {
        name: "subagent-driven-development",
        content: r#"---
name: "subagent-driven-development"
description: "Use when executing implementation plans with independent tasks in the current session — dispatches isolated workers via delegate_task, dispatch_subagent, invoke_subagent, or power_worktree_task with warm resume_id continuation, enforces the 4-Status Report Contract (DONE, DONE_WITH_CONCERNS, NEEDS_CONTEXT, BLOCKED), and runs two-stage power_review."
category: "workflow"
kind: "workflow"
globs: [".minicode/plan/task_plan.md", ".minicode/sdd/**/*", "minikit_docs/core/todo.md", ".minicode/notepads/session_wisdom.md"]
triggers: ["subagent-driven-development", "power_worktree_task", "subagent workflow", "two-stage review", "worker subagent", "delegate_task", "dispatch_subagent", "resume_id"]
allowed_tools: ["delegate_task", "dispatch_subagent", "invoke_subagent", "fanout_subagents", "execute_task_dag", "manage_subagents", "merge_subagent_worktree", "power_worktree_task", "power_review", "power_verify", "update_progress", "record_decision"]
always_apply: false
---

# Subagent-Driven Development (MiniPower / obra/superpowers v7.0.1)

## Core Orchestrator Workflow
1. **Pre-Flight Interface Scan & SDD Workspace**: Read `.minicode/plan/task_plan.md` and `.minicode/sdd/<slug>/progress.md` once at the start. Verify that every task's `Consumes` and `Produces` interfaces align and have zero file-ownership conflicts before dispatching any worker.
2. **Curated Self-Contained Task Brief**: Never force a worker to read the entire conversation history.
   - Use `delegate_task` / `dispatch_subagent` / `invoke_subagent` (with `run_in_background: true` for non-blocking background execution or `workspace_mode: "worktree"` for isolated file mutations) or `power_worktree_task(task=..., id=..., target_files=[...], acceptance_criteria=[...], consumes=[...], produces=[...], test_command=...)`.
   - **Warm Session Continuation (`resume_id`)**: When a completed worker needs follow-up fixes or answers to a question, pass `resume_id: "<subagent_id>"` to `delegate_task` / `dispatch_subagent` / `invoke_subagent` (or call `manage_subagents(action="resume", id="<subagent_id>", prompt="...")`) to reuse its warm in-memory conversation history instead of cold-spawning a new worker that re-reads the repository.
3. **Worker Constraints**:
   - **No-Subagents Contract**: Implementer and reviewer subagents MUST NOT recursively spawn subagents when at depth limit.
   - **4-Status Report Contract (`WorkerReportStatus`)**: Workers must conclude with `Status: DONE`, `Status: DONE_WITH_CONCERNS`, `Status: NEEDS_CONTEXT`, or `Status: BLOCKED`. Reusable codebase gotchas are automatically appended to `.minicode/notepads/session_wisdom.md` and relayed to subsequent workers.
4. **Handle Worker Status Deterministically**:
   - `DONE`: Proceed to two-stage review via `power_review` (or merge isolated worktree via `merge_subagent_worktree`).
   - `DONE_WITH_CONCERNS`: Read concerns; if correctness/scope is affected, resume the worker via `resume_id` before review; if observational, log via `record_decision`.
   - `NEEDS_CONTEXT`: Supply the requested file/interface context and resume the warm worker immediately via `delegate_task(resume_id="<id>", task="...")`.
   - `BLOCKED`: Assess whether to supply context via `resume_id`, decompose into smaller tasks, or escalate.
5. **Two-Stage Review (`power_review`) + 5-Round Circuit Breaker**:
   - Run `power_review(task_name=..., stage="both", base_sha=..., head_sha=...)`. Reviewers must NOT re-run tests the implementer already passed.
   - For follow-up fixes, resume the implementer with `resume_id` or `power_worktree_task(..., fix_findings=[...])` followed by `power_review(task_name=..., stage="re-review", findings=[...], fix_round=N)`.
   - **Circuit Breaker**: Never exceed **5 fix-and-re-review rounds** (`MAX_FIX_LOOP_ROUNDS = 5`).
"#,
    },
    BuiltinSkillDef {
        name: "dispatching-parallel-agents",
        content: r#"---
name: "dispatching-parallel-agents"
description: "Use when facing 2+ independent tasks or failure domains that can be worked on without shared state or overlapping file edits — dispatches concurrent workers via fanout_subagents, multiple parallel delegate_task calls, or execute_task_dag."
category: "workflow"
kind: "workflow"
globs: ["**/*"]
triggers: ["dispatching-parallel-agents", "fanout_subagents", "parallel agents", "parallel debugging", "concurrent tasks", "await_next"]
allowed_tools: ["fanout_subagents", "delegate_task", "dispatch_subagent", "invoke_subagent", "execute_task_dag", "manage_subagents", "merge_subagent_worktree", "power_worktree_task", "power_verify"]
always_apply: false
---

# Dispatching Parallel Agents (MiniPower / obra/superpowers v7.0.1)

## When to Use (`fanout_subagents` / Concurrent `delegate_task`)
Use `fanout_subagents` (or emit multiple `delegate_task` / `dispatch_subagent` calls in a single assistant turn) when you have **2 or more independent subsystems, modules, or failure domains** where investigating or fixing one does not touch the same files or shared state as the others.
- **Native Same-Turn Concurrency**: Read-only roles (`scout`, `explorer`, `researcher`, `reviewer`, `verifier`), background dispatches (`run_in_background: true`), and worktree-isolated workers (`workspace_mode: "worktree"`) execute in parallel inside the same tool batch!

## Parallel Dispatch Protocol
1. **Group by Independent Domain**: Partition failures or tasks into disjoint domains with explicit **File Ownership Boundaries** (`allowed_files` / `Produces`).
2. **Choose Blocking vs Reactive Non-Blocking Fan-Out**:
   - **Synchronous Barrier (`fanout_subagents(tasks=[...], run_in_background=false)`)**: Runs all workers concurrently up to pool capacity and returns a synthesized `SubagentSynthesisReducer` report when all complete.
   - **Non-Blocking Reactive Fan-Out (`fanout_subagents(tasks=[...], run_in_background=true)` or `delegate_task(..., run_in_background=true)`)**: Returns all `subagent_id`s immediately in `<1ms` so you can continue local coordinator work. Completed background workers are automatically injected into your context before the next turn inside `<completed_background_subagents>`, or you can wake on the **first** worker that finishes via `manage_subagents(action="await_next")`.
3. **Isolate & Integrate**: Use `workspace_mode: "worktree"` for concurrent write tasks, merge verified branches via `merge_subagent_worktree`, and run `power_verify` across the unified workspace.
"#,
    },
    BuiltinSkillDef {
        name: "verification-before-completion",
        content: r#"---
name: "verification-before-completion"
description: "Use when about to claim work is complete, fixed, or passing, before committing or creating PRs — requires running power_verify and reading full output to confirm 0 failures before making any success claims."
category: "testing"
kind: "workflow"
globs: [".minicode/plan/last_verify.json"]
triggers: ["verification-before-completion", "power_verify", "verify", "done", "complete", "tests pass", "build succeeds"]
allowed_tools: ["power_verify", "exec_cmd", "read_file", "lsp_diagnostics"]
always_apply: true
---

# Verification Before Completion (MiniPower / obra/superpowers v7.0.1)

## The Iron Law
```
NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE
```
If you have not run `power_verify` (or the exact verification command) in this message after your final edit, you cannot claim tests pass or the task is complete.

## The 5-Step Gate Function
1. **IDENTIFY**: Determine the exact command that proves your claim (`power_verify` auto-detects `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, or accepts explicit `files`).
2. **RUN**: Execute `power_verify` (`fresh`, complete, after the last file modification).
3. **READ**: Inspect the full output, exit code, compiler warnings, and `.minicode/plan/last_verify.json`.
4. **VERIFY**: Confirm the output actually proves the requirement (0 failed tests, 0 build errors).
5. **CLAIM**: Only now state completion, citing the exact verification command and result.

## Red Flags (Never Say Without Fresh `power_verify` Output)
- Ban phrases like *"should pass now"*, *"looks good"*, *"seems fixed"*, or *"trivial change, no need to re-run tests"*.
"#,
    },
    BuiltinSkillDef {
        name: "requesting-code-review",
        content: r#"---
name: "requesting-code-review"
description: "Use when completing tasks, implementing major features, or before merging to verify work meets requirements — runs two-stage power_review (Stage 1 Spec Compliance, Stage 2 Code Quality, and Scoped Re-Review) with review package artifacts and 'Declined to judge' surfacing."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["requesting-code-review", "power_review", "code review", "spec review", "quality review", "self-review"]
allowed_tools: ["power_review", "git_diff", "git_status", "read_file", "delegate_task", "record_decision"]
always_apply: false
---

# Requesting Code Review (MiniPower / obra/superpowers v7.0.1)

## Two-Stage Review Protocol (`power_review`)
Call `power_review(task_name=..., stage="both"|"spec"|"quality"|"re-review", base_sha=..., head_sha=..., acceptance_criteria=[...], findings=[...], fix_round=1)` after every task in `subagent-driven-development` or before `finishing-a-development-branch`:
1. **Compute Accurate Base Commit**: Determine the diff range from `git merge-base` against the target branch rather than assuming `HEAD~1`, and pass `base_sha` and `head_sha` so `power_review` writes `.minicode/sdd/<slug>/review-<base7>..<head7>.diff`.
2. **Stage 1: Spec Compliance (`stage: "spec"` or `"both"`)**:
   - Checks every requirement in `<docs_dir>/core/spec.md` and `.minicode/plan/task_plan.md` against the actual implementation line-by-line.
   - **The Spec Is a Vision Document**: For inputs or failure modes the spec is silent on, judge by what a reasonable user expects — a crash or data corruption on an unnamed input is Critical/Important, never Minor.
3. **Stage 2: Code Quality (`stage: "quality"` or `"both"`)**:
   - Evaluates error handling (zero `.unwrap()`/`.expect()` in production Rust), security, data layout, test falsifiability (`tdd-workflow`), and maintainability.
   - **Do Not Re-Run Passing Implementer Tests**: Reviewers inspect code and diffs; they do not waste tokens re-running the test suite the worker already verified.
   - **Declined to Judge**: Reviewers must explicitly list any domain or external system contract they could not verify so the orchestrator or human can inspect it.
4. **Scoped Re-Review (`stage: "re-review"`, `fix_round: 1..5`)**:
   - After a fix round, verify each prior finding (`ADDRESSED` vs `NOT ADDRESSED`) and inspect only the fix diff for new breakage (max 5 rounds).
"#,
    },
    BuiltinSkillDef {
        name: "receiving-code-review",
        content: r#"---
name: "receiving-code-review"
description: "Use when receiving code review feedback from humans or power_review subagents, before implementing suggestions — requires technical verification against the codebase, YAGNI filtering, and anti-sycophancy discipline."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["receiving-code-review", "review feedback", "address review", "pr comments", "fix findings"]
allowed_tools: ["read_file", "grep_search", "locate_symbol", "patch_file", "power_verify", "record_decision"]
always_apply: false
---

# Receiving Code Review with Technical Rigor (MiniPower / obra/superpowers v7.0.1)

## The Response Pattern
```
READ -> UNDERSTAND -> VERIFY IN CODEBASE -> EVALUATE (YAGNI?) -> RESPOND OR IMPLEMENT ONE AT A TIME
```

## Core Rules
1. **Zero Performative Agreement**: Never reply *"You're absolutely right!"* or *"Great catch!"*. State the technical fact of what changed or why a suggestion is declined.
2. **Verify Before Implementing**: External reviewers and review subagents can lack full repository context. Check `read_file`, `grep_search`, and `locate_symbol` to confirm whether the suggestion breaks existing callers, violates an active `[D1]..[DN]` decision in `.minicode/decisions.md`, or targets dead code.
3. **YAGNI Check**: If a reviewer suggests adding a speculative abstraction, config knob, or unused endpoint, grep for actual callers. If nothing calls it, push back with technical reasoning.
4. **Clarify Unclear Items First**: If 2 out of 5 review items are ambiguous, ask for clarification before implementing the interdependent items.
5. **Verify Each Fix**: Implement valid fixes one at a time and run `power_verify` to confirm no regressions.
"#,
    },
    BuiltinSkillDef {
        name: "using-git-worktrees",
        content: r#"---
name: "using-git-worktrees"
description: "Use when starting feature work that needs workspace isolation or before executing parallel/subagent plans — manages isolated git worktrees via power_worktree_task, delegate_task(workspace_mode='worktree'), and merge_subagent_worktree."
category: "workflow"
kind: "workflow"
globs: [".worktrees/**/*", ".minicode/worktrees/**/*"]
triggers: ["using-git-worktrees", "worktree", "power_worktree_task", "merge_subagent_worktree", "isolated branch"]
allowed_tools: ["power_worktree_task", "delegate_task", "manage_subagents", "merge_subagent_worktree", "power_verify", "git_status", "exec_cmd"]
always_apply: false
---

# Using Git Worktrees (MiniPower / obra/superpowers v7.0.1)

## Worktree Isolation Protocol
1. **Create Isolated Worktree**: Use `power_worktree_task` or `delegate_task(workspace_mode="worktree")` to create an isolated git worktree on a dedicated feature/task branch so the main workspace remains clean.
2. **Directory & Ignore Safety**: If using project-local worktrees (`.worktrees/` or `.minicode/worktrees/`), ensure the directory is git-ignored before creating files.
3. **Verify Clean Baseline**: Immediately run `power_verify` inside the new worktree to confirm tests pass at baseline before writing any new code. If baseline tests fail, report the pre-existing failure before proceeding.
4. **Inspect & Merge**: Use `manage_subagents(action="list")` to inspect active worktrees, and `merge_subagent_worktree` to integrate verified changes back into the target branch after confirming zero uncommitted or untracked work remains.
"#,
    },
    BuiltinSkillDef {
        name: "finishing-a-development-branch",
        content: r#"---
name: "finishing-a-development-branch"
description: "Use when implementation is complete and all tests pass via power_verify, to decide how to integrate work — surfaces [D1]..[DN] rulings and deferred SDD review minors, presents completion options, and safely cleans up worktrees."
category: "workflow"
kind: "workflow"
globs: [".minicode/plan/last_verify.json", ".minicode/decisions.md", ".minicode/sdd/**/*"]
triggers: ["finishing-a-development-branch", "finish branch", "merge branch", "complete feature", "archive_plan"]
allowed_tools: ["power_verify", "power_review", "read_file", "git_status", "git_commit", "merge_subagent_worktree", "archive_plan", "ask_user", "exec_cmd"]
always_apply: false
---

# Finishing a Development Branch (MiniPower / obra/superpowers v7.0.1)

## Step-by-Step Completion Protocol
1. **Verify Tests (`power_verify`)**: Run `power_verify` and confirm `.minicode/plan/last_verify.json` shows `passed: true`. Never proceed with failing tests.
2. **Surface Rulings & Deferred SDD Review Notes**:
   - Read `.minicode/decisions.md` and `.minicode/sdd/*/progress.md` and present a concise summary of all autonomous rulings (`[D1]..[DN]`), deferred `minor` findings, and any `"Declined to judge"` items from `power_review`.
3. **Present 3 Standard Options (via `ask_user` when interactive)**:
   1. **Merge back to `<base-branch>` locally** (`merge_subagent_worktree` + run `power_verify` on merged result).
   2. **Push and create a Pull Request** (`git push -u origin <branch>` + `gh pr create`).
   3. **Keep the branch/worktree as-is** (do not remove worktree).
   *(Note: Only execute a **Discard** flow if the user explicitly requests discarding the branch, and require typed confirmation naming the branch + commit count before deleting).*
4. **Safe Worktree & Plan Cleanup**:
   - Archive completed plan state via `archive_plan`.
   - Before removing a worktree, run `git status --porcelain` in the worktree to confirm zero uncommitted or untracked files exist. Never force-delete (`--force`) a worktree containing untracked user artifacts without explicit confirmation.
"#,
    },
    BuiltinSkillDef {
        name: "writing-skills",
        content: r#"---
name: "writing-skills"
description: "Use when creating new skills, editing existing skills in .minicode/skills/, or verifying skills work before deployment — applies RED-GREEN-REFACTOR TDD to skill documentation using pressure scenarios, CSO trigger descriptions, and <20KB compaction limits."
category: "workflow"
kind: "workflow"
globs: [".minicode/skills/**/*", "**/SKILL.md", "src/vault/builtin.rs"]
triggers: ["writing-skills", "create skill", "vault_create", "kit_skill_install", "skill tdd", "cso"]
allowed_tools: ["vault_create", "vault_load", "vault_search", "kit_skill_install", "delegate_task", "read_file", "write_file", "patch_file"]
always_apply: false
---

# Writing Skills via TDD (MiniPower / obra/superpowers v7.0.1)

## The Iron Law of Skill Authoring
```
NO SKILL WITHOUT A FAILING BASELINE PRESSURE SCENARIO FIRST
```
Creating a skill in `.minicode/skills/` (`vault_create`) is process TDD:
1. **RED — Baseline Pressure Scenario**: Design a realistic prompt combining 3+ pressures (sunk cost, time pressure, exhaustion) and observe how an agent rationalizes skipping the rule without the skill.
2. **GREEN — Write Minimal Skill (`vault_create`)**:
   - **CSO (Claude/Agent Search Optimization)**: The YAML `description` field must start with `"Use when..."` and describe **only triggering conditions and symptoms** — never summarize the workflow steps inside `description` (otherwise agents follow the short description and skip loading the full skill).
   - **Context Compaction Survival Limit**: Keep every `SKILL.md` **under 20,000 bytes** (`<500` lines). Move heavy reference tables or scripts to separate files so the core `SKILL.md` is never truncated during context compaction.
3. **REFACTOR — Close Loopholes**: Re-run the pressure scenario with the skill loaded (`vault_load`), capture any new rationalizations verbatim, and add them to the skill's Red Flags table.
"#,
    },
    BuiltinSkillDef {
        name: "diagnosing-minipower",
        content: r#"---
name: "diagnosing-minipower"
description: "Use when a MiniPower / superpowers workflow run went wrong, the user asks why an agent skipped a gate, stalled, repeated work, or drifted from the spec, or when auditing session transcripts via power_diagnose."
category: "quality"
kind: "workflow"
globs: [".minicode/plan/**/*", ".minicode/sdd/**/*", ".minicode/sessions/**/*", ".minicode/decisions.md", "minikit_docs/core/**/*"]
triggers: ["diagnosing-minipower", "diagnosing-superpowers", "power_diagnose", "session-forensics", "why did you skip", "workflow audit"]
allowed_tools: ["power_diagnose", "read_file", "grep_search", "file_search", "delegate_task"]
always_apply: false
---

# Diagnosing MiniPower Sessions (MiniPower / obra/superpowers v7.0.1)

## 7-Dimension Session Forensics (`power_diagnose`)
When a workflow run goes off the rails or the user reports a failure, invoke `power_diagnose(complaint)` first. `power_diagnose` streams `.minicode/sessions/*.jsonl` alongside `.minicode/plan/task_plan.md`, `.minicode/sdd/*/progress.md`, `.minicode/plan/last_verify.json`, `.minicode/decisions.md`, and `<docs_dir>/core/todo.md`, and computes the **Primary Investigation Lead** from actual transcript anomalies across all 7 dimensions:
1. **`skill-timeline`**: Were `power_brainstorm` -> `power_plan` -> `power_review` -> `power_verify` invoked in the mandated order, or was a gate skipped?
2. **`plan-adherence`**: Did completed tasks match `<docs_dir>/core/spec.md`, `.minicode/plan/task_plan.md`, `.minicode/sdd/<slug>/progress.md`, and their `Consumes`/`Produces` interfaces?
3. **`request-conflicts`**: Did user instructions conflict with skill gates or earlier `[D1]..[DN]` rulings in `.minicode/decisions.md`?
4. **`quality-evidence`**: Was `power_verify` actually run after the last file mutation, and were `power_review` Stage 1 & Stage 2 findings resolved?
5. **`repeated-work`**: Were identical `(tool, target)` pairs invoked `>= 3` times in loops?
6. **`stumbles`**: Did tool errors (`success=false`), worktree collisions, or subagent status (`NEEDS_CONTEXT` / `BLOCKED`) cause thrashing?
7. **`cost-and-time`**: Turn count, tool call count, subagent dispatches, slowest tool execution, token usage, and search cache utilization.

## Mandatory Citation Invariant
Every finding in your diagnostic report MUST cite an exact `path:line` from the session `.jsonl` transcript or workspace artifacts. Never speculate without transcript or artifact evidence.
"#,
    },
    // =========================================================================
    // MiniCode Orchestrator & Subsystem Capability Skills (Full CRUD Managed)
    // =========================================================================
    BuiltinSkillDef {
        name: "ask-user-inquiry",
        content: r#"---
name: "ask-user-inquiry"
description: "Use whenever you need clarity, domain data, or information from the user, or when you do not know or lack confidence about what to build — never guess or assume; ask as many questions as needed with 4 to 6 options via ask_user."
category: "orchestrator"
kind: "workflow"
globs: [".minicode/decisions.md", "minikit_docs/core/spec.md"]
triggers: ["ask-user-inquiry", "ask_user", "clarify", "trade-offs", "requirements", "ambiguous", "decision", "uncertain"]
allowed_tools: ["ask_user", "record_decision", "read_file", "grep_search", "locate_symbol"]
always_apply: true
---

# Socratic User Inquiry & Zero-Guessing Protocol (`ask-user-inquiry`)

## Core Principle: Never Guess or Assume When Uncertain
Whenever you need clarity, domain data, or information from the user—or whenever you do not know or lack confidence about what to do—do NOT guess or silently assume. Use `ask_user` to get explicit direction from the user.

1. **Step 1 — Check Discoverable Workspace Facts First**:
   - Inspect the repository via `read_file`, `grep_search`, `locate_symbol`, and `.minicode/decisions.md`.
   - Do not re-ask what the existing codebase, manifest (`package.json`, `Cargo.toml`, `minikit.json`), or recorded decisions already answer.
2. **Step 2 — Ask Whenever You Need Clarity, Data, or Confidence (`ask_user`)**:
   - Call `ask_user` for any unclear requirement, missing domain/business data, credentials (`input_type="secret"`), tech stack/framework choice, database/auth topology, visual aesthetic direction, product goals, or implementation trade-offs.
   - **Unlimited Questions**: Ask **as many questions as you need** in the `questions` array so every open uncertainty is resolved thoroughly.
   - **4 to 6 Options Per Choice Question**: Provide **4 to 6 distinct, well-explained options** for each choice question so the user has a rich set of concrete alternatives to pick from (plus the automatic custom write-in field).
   - **Recommended Option First**: Put your recommended choice first with `recommended: true` and a 1-line engineering rationale.
3. **Step 3 — Persist Every Settled Decision (`record_decision`)**:
   - Record settled choices in `.minicode/decisions.md` via `record_decision` (`[D1]..[DN]`) so neither you nor subagents re-ask them.
"#,
    },
    BuiltinSkillDef {
        name: "todo-planning",
        content: r#"---
name: "todo-planning"
description: "Use when creating, updating, reconciling, or archiving two-tier project roadmaps (minikit_docs/core/todo.md) and active tactical execution plans (.minicode/plan/task_plan.md)."
category: "orchestrator"
kind: "workflow"
globs: [".minicode/plan/task_plan.md", "minikit_docs/core/todo.md"]
triggers: ["todo-planning", "create_plan", "update_progress", "archive_plan", "todo.md", "task_plan.md", "roadmap", "checklist"]
allowed_tools: ["create_plan", "power_plan", "update_progress", "archive_plan", "record_decision", "read_file", "patch_file", "kit_sync"]
always_apply: true
---

# Two-Tier Todo & Tactical Plan Orchestration (`todo-planning`)

## Overview
`minicode` separates strategic project milestones from live tactical execution steps so both the `/todo` modal and the TUI live progress dock remain accurate.

## Tier 1: Strategic Project Roadmap (`minikit_docs/core/todo.md`)
- Tracks high-level architectural phases, feature milestones, and backlog items (`[ ]`, `[>]`, `[x]`).
- Inspect via `read_file(path="minikit_docs/core/todo.md")` and update phase checkboxes via `patch_file` or `power_plan`.

## Tier 2: Tactical Step Plan (`.minicode/plan/task_plan.md`)
- Created before multi-step or multi-file mutations via `create_plan` or `power_plan`.
- **Target File Attribution**: Every step MUST name its explicit target file(s) (e.g. `Implement JWT verifier in src/auth/jwt.rs`) so WorkingMemory bipartite file-to-step reconciliation and review diffs map deterministically.
- **Live Single-Active-Step Invariant**:
  - Maintain strictly one active `[>]` step at a time.
  - Call `update_progress(step="active", status="completed")` (or `completed_step` / `next_step`) immediately after completing each step's deliverable—never batch all `update_progress` calls to the end of the turn.
- **Completion & Archival**:
  - After all steps complete and pass `power_verify` + `power_review`, call `archive_plan` to archive the completed tactical plan cleanly.
"#,
    },
    BuiltinSkillDef {
        name: "miniblocks-ui",
        content: r#"---
name: "miniblocks-ui"
description: "Use when designing, scaffolding, styling, or saving reusable UI components, layouts, color palettes, and gradients with the MiniBlocks warehouse."
category: "ui-styling"
kind: "workflow"
globs: ["**/*.tsx", "**/*.jsx", "**/*.html", "**/*.css", "**/*.vue", "**/*.svelte"]
triggers: ["miniblocks-ui", "miniblocks", "block_search", "block_palettes", "block_scaffold", "block_insert", "block_save", "ui component", "color palette"]
allowed_tools: ["block_palettes", "block_gradients", "block_search", "block_get", "block_scaffold", "block_insert", "block_import", "block_save", "block_update", "block_delete"]
always_apply: false
---

# MiniBlocks UI Component & Art Direction Warehouse (`miniblocks-ui`)

## Core Workflow
1. **Art Direction & Palette Selection First (`block_palettes` + `block_gradients`)**:
   - Before writing CSS or Tailwind classes, query `block_palettes` (and `block_gradients` when appropriate) to select a cohesive design token palette (background primary/secondary, text hierarchy, accent primary/secondary, semantic status tokens, and WCAG AA contrast).
2. **Search & Scaffold Before Hand-Coding (`block_search` + `block_get` + `block_scaffold`)**:
   - Query `block_search(query=..., category=..., include_code=true)` across categories (`hero`, `navbar`, `bento`, `pricing`, `cards`, `forms`, `dashboard`, `footer`, `animations`, `charts`, `modals`, `tables`).
   - Use `block_scaffold` to assemble multi-block page layouts or `block_insert` to inject a specific block into a target file.
3. **Adapt & Evolve the Warehouse (`block_save` + `block_update` + `block_import` + `block_delete`)**:
   - Customize scaffolded blocks to match the project's domain data, typography, and brand tokens.
   - When you craft a high-quality reusable UI component or pattern, persist it via `block_save` (or refine an existing block via `block_update`, import external registries via `block_import`, or remove obsolete blocks via `block_delete`).
"#,
    },
    BuiltinSkillDef {
        name: "minipubs-delegation",
        content: r#"---
name: "minipubs-delegation"
description: "Use when delegating specialized research, planning, coding, UI design, testing, debugging, or security audits to the 14 MiniPubs specialist subagents, parallel worker waves, or reactive dependency DAGs."
category: "orchestrator"
kind: "workflow"
globs: ["**/*"]
triggers: ["minipubs-delegation", "minipub", "minipubs", "spawn_minipub", "update_minipub", "subagent", "delegate_task", "dispatch_subagent", "fanout_subagents", "execute_task_dag", "delegate"]
allowed_tools: ["delegate_task", "dispatch_subagent", "invoke_subagent", "fanout_subagents", "execute_task_dag", "manage_subagents", "merge_subagent_worktree", "minitask", "power_worktree_task"]
always_apply: false
---

# MiniPubs Specialist Subagent & Parallel Orchestration (`minipubs-delegation`)

## The 14 Specialist MiniPub Roles
- **Discovery & Architecture**: `explorer` / `scout` (fast read-only reconnaissance), `researcher` (deep codebase + web synthesis), `planner` (implementation breakdown), `architect` (system boundaries & interfaces).
- **Implementation & Design**: `coder` / `implementer` (focused feature/module implementation), `ui_designer` (frontend craft & MiniBlocks composition), `refactorer` (structural AST cleanup), `devops` (CI/CD, Docker, scripts), `docs` (technical documentation).
- **Verification & Hardening**: `reviewer` (adversarial spec & quality audit), `tester` / `verifier` (falsifiable RED->GREEN test authoring), `debugger` (root-cause tracing), `security` (OWASP & boundary audit), `performance` (algorithmic & latency optimization).

## Canonical Subagent & Lifecycle Operations
1. **Direct & Non-Blocking Delegation (`delegate_task` / `dispatch_subagent` / `invoke_subagent`)**:
   - Pass `run_in_background: true` for non-blocking background execution or `resume_id: "<subagent_id>"` for warm session continuation.
   - Or spawn with live checklist tracking via `minitask(action="spawn_minipub", role="<role>", task="...", todos=["step 1", "step 2"], until="...", max_turns=20)`.
2. **Event-Driven Synchronization (`manage_subagents` / `minitask`)**:
   - Completed background subagents automatically inject their `<completed_background_subagents>` Boomerang report at the next turn boundary.
   - Wake immediately when the **first** background worker finishes via `manage_subagents(action="await_next")` (or wait for all via `manage_subagents(action="wait")`).
   - Resume a completed worker with its warm conversation history intact via `manage_subagents(action="resume", id="<subagent_id>", prompt="...")`.
3. **Zero-Copy Artifact Relaying & Historical Slicing**:
   - Reference `$RESULT[pub_id]` (or `$RESULT[latest:role]`) in downstream worker prompts to forward full output without copying large payloads through the coordinator.
   - Reference `$TURN[pub_id:N]` to inspect a specific turn from a worker's execution log.
4. **Parallel Waves (`fanout_subagents`) & Reactive DAGs (`execute_task_dag`)**:
   - Use `fanout_subagents(tasks=[...], run_in_background=true|false)` when 2+ tasks have disjoint file ownership boundaries, or `execute_task_dag(tasks=[...])` when tasks form a multi-wave dependency graph (`depends_on`).
"#,
    },
    BuiltinSkillDef {
        name: "async-dag-orchestration",
        content: r#"---
name: "async-dag-orchestration"
description: "Use when orchestrating multi-wave dependency graphs, non-blocking background subagents, event-driven await_next synchronization, or warm subagent session continuation (resume_id)."
category: "orchestrator"
kind: "workflow"
globs: [".minicode/plan/task_plan.md", ".minicode/notepads/session_wisdom.md"]
triggers: ["async-dag-orchestration", "execute_task_dag", "await_next", "wait_any", "resume_id", "background subagent", "boomerang", "dag orchestration"]
allowed_tools: ["execute_task_dag", "fanout_subagents", "delegate_task", "dispatch_subagent", "invoke_subagent", "manage_subagents", "merge_subagent_worktree"]
always_apply: false
---

# Reactive Event-Driven Async DAG & Non-Blocking Orchestration (`async-dag-orchestration`)

## 1. True Same-Turn Concurrency & Non-Blocking Background Dispatch
- Emit multiple `delegate_task` / `dispatch_subagent` / `invoke_subagent` calls in a single assistant turn (or call `fanout_subagents(tasks=[...], run_in_background=true)`):
  - Read-only roles (`scout`, `explorer`, `researcher`, `reviewer`, `verifier`), background workers (`run_in_background: true`), and worktree-isolated workers (`workspace_mode: "worktree"`) execute concurrently in the same batch.
- With `run_in_background: true`, the tool returns `subagent_id` in `<1ms` so you can continue local coordinator work while workers run in the background.

## 2. Mid-Turn Reactive Boomerang Inbox & Event-Driven `await_next`
- Whenever background workers complete while you are working, their structured `<completed_background_subagents>` Boomerang reports are automatically injected before your next LLM turn — never poll `manage_subagents(action="list")` in a busy loop.
- When you want to block until the **first** background worker finishes, call `manage_subagents(action="await_next")` (`wait_any`), which wakes on the `SubagentPool` completion condvar the millisecond any background worker finishes.

## 3. Reactive Topological DAG Wave Execution (`execute_task_dag`)
- When tasks have upstream/downstream dependencies (`t1`, `t2` -> `t3`), call `execute_task_dag(tasks=[{id: "t1", role: "scout", task: "..."}, {id: "t2", role: "scout", task: "..."}, {id: "t3", role: "coder", task: "...", depends_on: ["t1", "t2"]}])`:
  - Validates topological ordering and disjoint write-file ownership (`allowed_files`).
  - Spawns all 0-indegree ready nodes concurrently on `SubagentPool`.
  - Wakes via `await_next_completion` as each upstream node completes, auto-injects upstream `$RESULT[dep_id]` summaries into newly unblocked downstream prompts, and spawns downstream nodes immediately without head-of-line blocking.

## 4. Warm Session Continuation (`resume_id`) & 4-Status Contract
- Resume any completed worker with its full in-memory conversation history preserved via `delegate_task(resume_id="<subagent_id>", task="...")` or `manage_subagents(action="resume", id="<subagent_id>", prompt="...")`.
- Every worker concludes with `Status: DONE | DONE_WITH_CONCERNS | NEEDS_CONTEXT | BLOCKED`, and reusable codebase learnings are relayed through `.minicode/notepads/session_wisdom.md`.
"#,
    },
    BuiltinSkillDef {
        name: "minitask-runtime",
        content: r#"---
name: "minitask-runtime"
description: "Use when starting or monitoring background dev servers, scheduling recurring cron jobs or one-shot timers, inspecting process telemetry, or running headless/GUI PageAgent browser interaction and QA verification."
category: "orchestrator"
kind: "workflow"
globs: ["**/*"]
triggers: ["minitask-runtime", "minitask", "dev server", "background process", "schedule", "timer", "cron", "page-agent", "browser_navigate", "browser_snapshot", "browser_click", "browser_fill", "browser_select_option", "browser_scroll", "browser_batch", "browser_qa_audit"]
allowed_tools: ["minitask", "browser_navigate", "browser_snapshot", "browser_click", "browser_fill", "browser_select_option", "browser_eval", "browser_hover", "browser_scroll", "browser_scroll_horizontal", "browser_batch", "browser_inspect_dom", "browser_qa_audit", "browser_aura", "browser_badges", "browser_metrics", "browser_screenshot", "browser_close"]
always_apply: false
---

# MiniTask Background Runtime, Async Builds/Tests, Scheduler & PageAgent Browser Verification (`minitask-runtime`)

## 1. Async Background Builds, Compilers, Test Suites & Scripts (`wait_ms_before_async` + `minitask`)
- All build, compile, test, bundle, install, and runner commands across any stack (`bun build`, `bun run`, `bun test`, `cargo build`, `cargo check`, `cargo test`, `npm install`, `npm run build`, `pytest`, `go build`, `go test`, `tsc`, `vite build`, `make`, `gradle`, etc.) are automatically supervised in `minitask` (`MiniDevRegistry`).
- Use `exec_cmd(command="...", wait_ms_before_async=2000)` or `exec_cmd(command="...", run_in_background=true)` or `minitask(action="run", command="...", wait_ms_before_async=1000)` to run builds/tests asynchronously without blocking your turn:
  - If the command finishes within `wait_ms_before_async`, its RTK-compacted + fault-localized output is returned immediately.
  - If it continues running past `wait_ms_before_async`, it runs in the background in `minitask` (streaming live output to `.minicode/logs/tasks/<id>.log`), and the system automatically notifies you with `<completed_background_tasks>` as soon as it finishes — **never poll in a loop**.
- Monitor or interact with running background tasks anytime via `minitask(action="status", id="<id>")`, `minitask(action="logs", id="<id>", tail=50)`, `minitask(action="await", id="<id>", wait_mode="all"|"any")`, `minitask(action="send_input", id="<id>", input="...")`, or `minitask(action="kill", id="<id>")`.

## 2. Background Dev Servers & Daemons (`is_daemon: true`)
- Never run long-lived servers (`npm run dev`, `vite`, `cargo watch`, `uvicorn`, `next dev`) inside synchronous blocking `exec_cmd`.
- Start them as persistent daemons via `minitask(action="start", command="...", name="dev-server")` (or `exec_cmd(command="...", is_daemon=true)`).
- Monitor readiness, logs, and CPU/RAM telemetry via `minitask(action="status")`, `minitask(action="logs", id="...")`, and `minitask(action="resources")`.

## 3. Timers & Cron Schedules
- Never use `sleep` in `exec_cmd` to wait or poll.
- Use `minitask(action="schedule", duration_seconds=..., prompt="...")` for one-shot delayed wakeups or `minitask(action="schedule", cron="...", prompt="...")` for recurring tasks.

## 3. Live PageAgent Browser E2E & Visual QA Verification
- Once a web server is listening, verify the live rendered application with `browser_navigate(url="http://localhost:...")` or `browser_snapshot` (which dehydrates the live DOM + Shadow DOM into versioned `@vX:eY` interactive refs, scrollability metrics, and masked sensitive fields).
- Exercise interactive flows via versioned `@vX:eY` refs or selectors using `browser_click`, `browser_fill`, `browser_select_option`, `browser_hover`, `browser_eval`, `browser_scroll` (viewport or scrollable container by `ref`/`selector`), `browser_scroll_horizontal`, or atomic multi-step `browser_batch`.
- Use `browser_aura` (SimulatorMask glowing border + animated cursor + HALT button), `browser_badges` (`[1]..[N]` element overlays), `browser_metrics` (viewport & scroll pagination), and `browser_inspect_dom` for visual debugging.
- Run `browser_qa_audit` and `browser_screenshot` to verify layout integrity, zero console errors, and zero failed network requests before closing with `browser_close`.
"#,
    },
    BuiltinSkillDef {
        name: "minikit-scaffolding",
        content: r#"---
name: "minikit-scaffolding"
description: "Use when bootstrapping a new project, inspecting or applying MiniKit starter stacks, checking architectural drift, synchronizing .minicode/minicode.md, or managing packages."
category: "orchestrator"
kind: "workflow"
globs: ["minikit.json", "onpkg.json", ".minicode/minicode.md", "minikit_docs/**/*"]
triggers: ["minikit-scaffolding", "kit_stack_list", "kit_stack_add", "kit_sync", "kit_add", "scaffold", "starter stack", "bootstrap"]
allowed_tools: ["kit_stack_list", "kit_stack_show", "kit_stack_add", "kit_stack_diff", "kit_sync", "kit_doctor", "kit_add", "kit_remove", "kit_info", "read_file", "patch_file"]
always_apply: false
---

# MiniKit Scaffolding, Workspace Sync & Package Management (`minikit-scaffolding`)

## 1. Starter Stack Discovery & Scaffolding
- When initializing a new project or empty workspace, check available stacks first with `kit_stack_list` and inspect details via `kit_stack_show(stack="<name>")`.
- Scaffold directly into the workspace using `kit_stack_add(stack="<name>", dir=".")` instead of hand-writing boilerplate config files from scratch.
- Check architectural drift against the stack template anytime via `kit_stack_diff`.

## 2. Canonical `.minicode/minicode.md` & Core Docs Synchronization
- `.minicode/minicode.md` is the single canonical workspace instruction manifest (never generate redundant `AGENTS.md` files).
- Keep `minikit_docs/core/` (`coreidea.md`, `prd.md`, `architecture.md`, `spec.md`, `design.md`, `implementation.md`, `todo.md`) and `.minicode/minicode.md` synchronized via `kit_sync` and `kit_doctor`.

## 3. Native Package Management & Continuous Learning
- Install or remove dependencies using `kit_add` and `kit_remove` (which automatically respect the workspace's configured `package_manager` in `minikit.json` / `onpkg.json`).
- Record package-specific patterns or troubleshooting fixes in `minikit_docs/packages/<pkg>.md`.
"#,
    },
    BuiltinSkillDef {
        name: "ast-codegraph-navigation",
        content: r#"---
name: "ast-codegraph-navigation"
description: "Use when exploring unfamiliar codebases, locating symbols, analyzing blast radius and call graphs, or performing structural AST search and multi-file refactoring."
category: "orchestrator"
kind: "workflow"
globs: ["**/*"]
triggers: ["ast-codegraph-navigation", "ast_grep_search", "ast_grep_rewrite", "locate_symbol", "hybrid_search", "code_explore", "code_trace", "code_impact", "diff_impact", "refactor"]
allowed_tools: ["locate_symbol", "hybrid_search", "semantic_search", "ast_query", "ast_extract_symbol", "ast_diff", "ast_grep_search", "ast_grep_rewrite", "code_explore", "code_explain", "code_trace", "code_impact", "diff_impact", "grep_search", "read_file"]
always_apply: false
---

# Structural AST, CodeGraph & Trigram Search Navigation (`ast-codegraph-navigation`)

## 1. Surgical Symbol Discovery Over Blind Full-File Reads
- Use `locate_symbol`, `hybrid_search`, `code_explore`, and `ast_extract_symbol` to jump directly to symbol definitions, signatures, and exact line ranges before calling `read_file(path, start_line, end_line)`.

## 2. Call-Graph & Blast-Radius Analysis
- Before modifying shared traits, structs, public functions, or API routes, run `code_trace`, `code_impact`, or `diff_impact` to map callers, callees, and affected tests across the repository.

## 3. Structural `ast-grep` vs Trigram `grep_search`
- **Use `ast_grep_search` & `ast_grep_rewrite`** for syntax-aware code queries and multi-file structural refactoring using metavariables (`$VAR`, `$$$ARGS`). Always preview rewrites with `dry_run=true` first, then apply with `dry_run=false`.
- **Use `grep_search`** (trigram-accelerated) for fast literal or regex searches across comments, log strings, error messages, Markdown docs, and configuration files (`.json`, `.toml`, `.yaml`, `.env`).
"#,
    },
    BuiltinSkillDef {
        name: "caveman",
        content: r#"---
name: "caveman"
description: "Use when the user asks to talk like caveman, use less tokens, be brief, or when Caveman Mode is active — enforces the 9-rule ASD-STE100 terse voice (~75% fewer output tokens) while preserving 100% technical precision."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["caveman", "talk like caveman", "less tokens", "fewer tokens", "terse", "concise"]
always_apply: false
---

# Caveman v4.0 Voice (`juliusbrussee/caveman` Rock 1)

## 9-Rule `ASD-STE100` Floor
1. **Answer First**: Start with the answer or action (`[thing] [action] [reason]. [next step].`).
2. **Kill Ceremony**: No `"Sure!"`, `"Happy to help"`, `"Let me explain"`, or `"In summary"`.
3. **Short Words**: Use `"fix"` not `"implement a solution for"`, `"big"` not `"extensive"`, `"use"` not `"utilize"`. Standard acronyms (`DB`, `API`, `HTTP`, `JSON`, `PR`, `CI`, `CLI`, `JWT`, `DOM`, `URL`) OK; do NOT invent code-shorthand (`cfg`/`impl`/`fn`) or arrows in `full` mode.
4. **Articles Optional, Negations Mandatory**: Drop `a`, `an`, `the`, and filler (`just`, `really`, `basically`, `actually`, `simply`). **NEVER drop `not`, `never`, `no`, `only`, `except`, `unless`**, or genuine uncertainty.
5. **One Idea Per Sentence (`ASD-STE100`)**: Max 20 words per sentence, active voice, imperative mood for instructions, noun clusters <=3 words.
6. **Payload Verbatim**: Code blocks, CLI commands, file paths, URLs, config keys, env vars, version numbers, git SHAs, error strings, and SQL stay 100% untouched.
7. **Bounded Tool-Run Status**: Emit **NO text between routine tool calls**. On completion, emit 1 sentence.
8. **User's Language**: Apply rules in the user's language.
9. **Never Perform Caveman**: No `"Me fix bug"` or `"fire good"`. Technical precision without filler.
"#,
    },
    BuiltinSkillDef {
        name: "ultracave",
        content: r#"---
name: "ultracave"
description: "Use when maximum English token compression is needed ('ultracave', 'caveman ultra') — compresses prose to dense technical fragments, standard shorthand (DB/auth/config/req/res/fn/impl), and causal arrows (-> / <-)."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["ultracave", "caveman ultra", "ultra caveman", "maximum compression"]
always_apply: false
---

# UltraCave Maximum Fragment Compression (`juliusbrussee/caveman`)

1. Emit dense technical fragments (`Inline object prop, new ref, re-render. useMemo.`).
2. Abbreviate prose words (`DB`, `auth`, `config`, `req`, `res`, `fn`, `impl`, `param`, `env`, `deps`) and use causal arrows (`X -> Y`, `X <- Y`).
3. Never abbreviate code symbols, function names, file paths, or error strings, and never drop `not`, `never`, `only`, or `except`.
"#,
    },
    BuiltinSkillDef {
        name: "megacave",
        content: r#"---
name: "megacave"
description: "Use when extreme English telegraphic & symbolic token-density mode is requested ('megacave', 'caveman mega') — responds in ultra-compact English telegraphic notation with ASCII operators (&, |, ->, <-, :=, !=, @) while keeping all code, paths, APIs, and errors 100% exact."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["megacave", "mega", "caveman-mega", "telegraphic"]
always_apply: false
---

# MegaCave Extreme English Telegraphic Register (`juliusbrussee/caveman` Mega Mode)

1. Encode causality, conjunctions, and scope in compact English keywords + ASCII operators (`&`, `|`, `->`, `<-`, `:=`, `!=`, `@`, `<=12` words/line; e.g. `root-cause: null ptr @ auth.rs:42 -> patch guard; test ok.`) for ~85-88% output token reduction.
2. Never drop negations or scope tokens (`not`, `never`, `no`, `only`, `except`, `unless`).
3. Keep all code blocks, identifiers, file paths, CLI commands, API names, and error messages 100% exact.
"#,
    },
    BuiltinSkillDef {
        name: "cavecrew",
        content: r#"---
name: "cavecrew"
description: "Use when dispatching or executing compressed subagent roles (cavecrew-investigator, cavecrew-reviewer, cavecrew-builder) to shrink subagent handoff payloads by ~65%."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["cavecrew", "cavecrew-investigator", "cavecrew-reviewer", "cavecrew-builder"]
always_apply: false
---

# CaveCrew Subagent Output Contracts (`juliusbrussee/caveman` Rock 4)

1. **`cavecrew-investigator`**: Emit `<Header>:\n- path:line — symbol — short technical note (<=12 words)\ntotals: <counts>.`
2. **`cavecrew-reviewer`**: Emit `path:line: <CRITICAL|HIGH|MED|LOW>: <problem>. <fix>.\ntotals: <counts>` (or `LGTM.`).
3. **`cavecrew-builder`**: Emit `<path:line-range> — <change in <=10 words>.\nverified: <pass | fail | unavailable | blocked> (<check/test result>).`
4. **Builder Bailout Tokens**: When blocked, stop immediately and emit one terminal token: `too-big: <reason>.`, `needs-confirm: <destructive action>.`, `ambiguous: <options>.`, or `regressed: <failing test>.`
"#,
    },
    BuiltinSkillDef {
        name: "caveman-explore",
        content: r#"---
name: "caveman-explore"
description: "Use when running fast read-only repository scouting ('FastContext') — fans out parallel search/read calls in turn 1 and replies ONLY with an evidence block ('path/to/file.ext:START-END  reason')."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["caveman-explore", "fastcontext", "fast-context", "evidence block"]
allowed_tools: ["locate_symbol", "hybrid_search", "grep_search", "ast_grep_search", "code_explore", "read_file"]
always_apply: false
---

# Caveman Explore (`FastContext` Evidence-Only Scout)

1. **Parallel Turn-1 Fanout**: Issue parallel `grep_search`, `locate_symbol`, `ast_grep_search`, and `read_file` calls in turn 1; follow extra hops only when imports or callers require it.
2. **Strict Evidence-Only Reply**: Return **ONLY** lines matching `path/to/file.ext:START-END  <reason it is relevant in <=12 words>` (or `no relevant locations found`). Emit zero preamble, zero headings, and zero code blocks.
"#,
    },
    BuiltinSkillDef {
        name: "caveman-commit",
        content: r#"---
name: "caveman-commit"
description: "Use when drafting git commit messages — enforces ultra-compressed Conventional Commits (<=50 chars subject, body only when non-obvious why or breaking change, zero AI attribution)."
category: "efficiency"
kind: "workflow"
globs: [".git/**/*"]
triggers: ["caveman-commit", "commit message", "git_commit"]
allowed_tools: ["git_status", "git_diff", "git_commit"]
always_apply: false
---

# Caveman Commit (`juliusbrussee/caveman`)

1. **Subject Line**: `<type>(<scope>): <imperative summary>` (`<=50` chars, hard cap `72`, no trailing period, imperative mood: `add`, `fix`, `remove`, `refactor`).
2. **Optional Body**: Omit body for 80% of commits. Include a bulleted body (`-` lines) ONLY for breaking changes, non-obvious *why*, or migration notes.
3. **Forbidden**: Never write `"This commit..."`, never restate file diffs in prose, and never add `Co-Authored-By` or AI attribution trailers unless explicitly requested.
"#,
    },
    BuiltinSkillDef {
        name: "caveman-review",
        content: r#"---
name: "caveman-review"
description: "Use when reviewing diffs or pull requests — emits single-line actionable findings ('<file>:L<line>: <severity>: <problem>. <fix>.') with zero throat-clearing."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["caveman-review", "code review", "pr review", "critic_review"]
allowed_tools: ["git_diff", "power_review", "critic_review", "read_file"]
always_apply: false
---

# Caveman Review (`juliusbrussee/caveman`)

1. **Format**: `<file>:L<line>: <CRITICAL|HIGH|MED|LOW>: <problem in <=10 words>. <concrete fix in <=10 words>.`
2. **Clean Diffs**: If no bugs, security flaws, or missing invariants exist, emit `LGTM.` and stop.
3. **Zero Fluff**: Never compliment code, summarize the PR, or hedge with `"Consider maybe..."`.
"#,
    },
    BuiltinSkillDef {
        name: "caveman-compress",
        content: r#"---
name: "caveman-compress"
description: "Use when compressing markdown documentation or memory files — applies AST-safe prose compression (~45-65% token reduction) while keeping headings, code blocks, links, tables, and negations 100% intact."
category: "efficiency"
kind: "workflow"
globs: [".minicode/**/*.md", "minikit_docs/**/*.md"]
triggers: ["caveman-compress", "compress markdown", "compress docs"]
always_apply: false
---

# Caveman Markdown AST Compressor (`juliusbrussee/caveman` Rock 2)

1. Compress only pure prose paragraphs and list items; keep all headings, fenced code blocks, inline code (`backticks`), URLs, tables, and custom XML workflow tags 100% untouched.
2. Never compress sensitive files (`.env*`, credentials, keys, source code) and never drop negation or scope tokens (`not`, `never`, `no`, `only`, `except`, `unless`).
"#,
    },
    BuiltinSkillDef {
        name: "caveman-learn",
        content: r#"---
name: "caveman-learn"
description: "Use when auditing session token waste or distilling repeated multi-tool procedures into skills/blocks — runs the 10-sink Caveman Waste Analyzer with a strict net-token-negative gate."
category: "efficiency"
kind: "workflow"
globs: [".minicode/sessions/**/*", ".minicode/blocks/**/*"]
triggers: ["caveman-learn", "caveman-stats", "caveman-optimize", "token waste", "optimize_token_budget"]
allowed_tools: ["optimize_token_budget", "power_diagnose", "vault_create"]
always_apply: false
---

# Caveman Learn — 10-Sink Token Waste & Skill Distillation (`juliusbrussee/caveman` Rock 5)

1. Call `optimize_token_budget` to inspect all 10 token waste sinks (`reread_waste`, `mcp_schema_tax`, `subagent_spend_share`, `cache_hygiene`, `compaction_churn`, `section_bloat`, `tool_output_portfolio`, `procedure_repeat` / `SKILL_DISTILLATION`, `outcome_efficiency`, and `memory_offload_health`).
2. **Strict Net-Token-Negative Gate**: Apply a fix, memory offload, or skill distillation ONLY when `projected_savings_tokens > fix_injection_cost_tokens`.
3. **Rock 6 Reusable Blocks**: When a multi-command shell verification pipeline repeats >=3 times, save it once via `.minicode/blocks/<name>.sh` and run the 1-line script on subsequent turns.
"#,
    },
    BuiltinSkillDef {
        name: "investigate-first",
        content: r#"---
name: "investigate-first"
description: "Use when diagnosing a bug or failure with multiple plausible causes — ranks hypotheses by evidence and falsifies the top hypothesis before editing product code."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["investigate-first", "root cause", "ambiguous failure", "falsify hypothesis"]
allowed_tools: ["grep_search", "locate_symbol", "code_trace", "read_file", "exec_cmd", "explore_hypotheses"]
always_apply: false
---

# Investigate First (`juliusbrussee/caveman` Rock 3 Native Core)

1. Restate observed symptom and exact failure location in 1 line.
2. List 2–3 ranked hypotheses with the fastest falsification check (`read_file`, `code_trace`, or reproducer command) for each.
3. Run the narrowest check to falsify or confirm #1 before touching any product code.
4. Once root cause is proven with `path:line` evidence, apply a minimal fix at the owning boundary.
"#,
    },
    BuiltinSkillDef {
        name: "lean-build",
        content: r#"---
name: "lean-build"
description: "Use when implementing a feature with high overbuilding risk — derives observable acceptance criteria and explicit non-goals, traces the entry point through invariant-owning layers, and omits speculative modes or config."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["lean-build", "anti-overbuilding", "minimal feature", "yagni"]
always_apply: false
---

# Lean Build (`juliusbrussee/caveman` Rock 3 Native Core)

1. State observable acceptance criteria and explicit non-goals in <=3 lines before writing code.
2. Trace the single entry point through the existing layers that already own the invariant; reuse existing structs/enums instead of inventing wrapper traits or speculative config flags.
3. Implement only what the acceptance check exercises, verify with the narrowest test, and stop.
"#,
    },
    BuiltinSkillDef {
        name: "safe-refactor",
        content: r#"---
name: "safe-refactor"
description: "Use when restructuring code while preserving existing external behavior — moves one ownership boundary at a time and brackets each step with verification."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["safe-refactor", "behavior-preserving", "restructure"]
allowed_tools: ["code_impact", "ast_grep_search", "ast_grep_rewrite", "patch_file", "exec_cmd", "power_verify"]
always_apply: false
---

# Safe Refactor (`juliusbrussee/caveman` Rock 3 Native Core)

1. Confirm baseline tests pass and map callers via `code_impact` / `code_trace` before editing.
2. Move or reshape **one ownership boundary at a time** — never mix behavior changes with structural refactoring in the same step.
3. Re-run the narrowest compiler/test check after each boundary move and stop as soon as the target structure compiles and passes.
"#,
    },
    BuiltinSkillDef {
        name: "surgical-patch",
        content: r#"---
name: "surgical-patch"
description: "Use when fixing a confirmed bug at a known root cause — applies the smallest possible change at the narrowest owning layer with a focused regression proof and zero drive-by edits."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["surgical-patch", "minimal fix", "hotfix", "narrow patch"]
allowed_tools: ["read_file", "patch_file", "exec_cmd", "power_verify"]
always_apply: false
---

# Surgical Patch (`juliusbrussee/caveman` Rock 3 Native Core)

1. Fix the bug at the narrowest layer that owns the broken invariant.
2. Touch zero unrelated lines, formatting, or comments (no drive-by refactoring).
3. Prove the fix with 1 focused regression test covering the exact failure input and stop immediately.
"#,
    },
    BuiltinSkillDef {
        name: "verify-and-stop",
        content: r#"---
name: "verify-and-stop"
description: "Use when validating completed edits — translates acceptance criteria into the smallest sufficient proof set, classifies status as pass/fail/unavailable/blocked, and stops immediately when proof passes."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["verify-and-stop", "proof set", "stop when green"]
allowed_tools: ["exec_cmd", "power_verify", "lsp_diagnostics"]
always_apply: false
---

# Verify and Stop (`juliusbrussee/caveman` Rock 3 Native Core)

1. Translate acceptance criteria into the **smallest sufficient proof command** (targeted unit test + compiler check).
2. Classify verification outcome strictly as `pass`, `fail`, `unavailable`, or `blocked`.
3. When outcome is `pass`, report the 1-line proof result and **stop immediately** — do not run redundant full-suite loops or add post-verification polish edits.
"#,
    },
    BuiltinSkillDef {
        name: "migration",
        content: r#"---
name: "migration"
description: "Use when performing schema, API, or dependency migrations — sequences compatibility bridges, data/caller cutover, and verification without breaking active callers."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["migration", "schema migration", "api migration", "dependency upgrade"]
allowed_tools: ["code_impact", "ast_grep_search", "ast_grep_rewrite", "patch_file", "exec_cmd", "power_verify"]
always_apply: false
---

# Safe Migration Workflow (`juliusbrussee/caveman` Rock 3 Native Core)

1. Map all callers/consumers of the old schema, API, or crate via `code_impact` and `grep_search`.
2. Introduce the target contract or compatibility bridge first, migrate callers deterministically (`ast_grep_rewrite` or scoped `patch_file`), and remove deprecated shims only after all callers pass verification.
"#,
    },
    BuiltinSkillDef {
        name: "ponytail",
        content: r#"---
name: "ponytail"
description: "Minimalist YAGNI coding discipline ('the laziest senior dev in the room'): 7-Rung Solution Ladder, Reuse Before Create via <reusable_symbol_map>, shortcut: <ceiling>, <when to upgrade> debt ledger, caller-aware bug fixes, and Honesty Footer."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["ponytail", "laziest senior dev", "reuse first", "shortcut:", "minimalist coding"]
allowed_tools: ["repo_map", "code_impact", "code_trace", "read_file", "patch_file", "power_verify"]
always_apply: false
---

# Ponytail Minimalist Engineering Discipline (`dietrichgebert/ponytail`)

> *"The best code is the code you never wrote."*

## 1. The 7-Rung Solution Ladder (Climb from Rung 1 -> 7)
1. **Delete / Config**: Can the problem be solved by deleting dead code or changing existing config? Do that.
2. **Reuse Existing Symbol**: Check `<reusable_symbol_map>` and `repo_map(mode="reuse")` first. Reuse existing project functions, structs, or components the way surrounding code does.
3. **Standard Library / Platform**: Use stdlib or native runtime features unless a project helper already exists.
4. **Installed Dependency**: Use an already-installed crate/package. Never add a new dependency for a few lines of logic.
5. **One-Liner**: If a clear, readable one-liner solves it at a glance, write one line.
6. **Minimum Working Code**: Write the smallest diff that satisfies the real requirement—no speculative wrappers, single-implementation traits, or unrequested config flags.
7. **Shortcut Comment + Proof Check**: When taking a deliberate YAGNI shortcut with a known scaling ceiling, annotate it in code as `// shortcut: <the limit>, <when to upgrade>` and verify non-trivial logic with one focused test.

## 2. Non-Negotiable Invariants
- **Be lazy about the solution, never about the change itself**: Finish every caller, test, and fixture your change touches (`open_callers == 0`).
- **Bug Fixes**: Before editing, trace every caller via `code_impact` / `grep_search` and fix the root cause once in shared code.
- **Never Cut**: Validation at trust boundaries, error handling that prevents data loss, security, accessibility, hardware calibration, or explicit user requirements.
- **Honesty Footer**: End turns with a 1-line note on any shortcut ceiling taken or `Not checked:` path that could not be verified.
"#,
    },
    BuiltinSkillDef {
        name: "ponytail-review",
        content: r#"---
name: "ponytail-review"
description: "Senior-dev quality review of a diff (bugs, security, real load, missing tests, speed, and what to delete): 4-part plain-language findings (What this is / Problem / Fix / If we skip it), Lean: -<N> lines possible, and Not checked: honesty footer."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["ponytail-review", "diff review", "lean review", "senior review"]
allowed_tools: ["git_diff", "git_status", "power_review", "critic_review", "code_impact", "code_trace", "read_file"]
always_apply: false
---

# Ponytail Diff Review (`dietrichgebert/ponytail`)

Review the diff like the senior engineer who gets paged when it breaks.
**Priority order**: Correct -> Safe -> Holds under load -> Tested -> Fast -> Lean.

## 1. Trace Before Reporting
- Read the diff, then inspect callers of every changed function (`code_impact`), the functions it calls (`code_trace`), and its tests.
- When a signature, return value, or invariant changes, verify every caller in the workspace.
- State the expected load you assumed (single-user CLI vs. concurrent multi-worker service).

## 2. 6-Pillar Inspection (`Look For`)
1. **Bug**: Wrong result, crash, missed edge case (empty, zero, last item, rounding, timezones), broken caller, or symptom patch applied in one caller while shared logic stays broken.
2. **Risk**: Security holes (injection, secrets, missing input validation) or data loss (swallowed errors, out-of-order writes, missing transaction).
3. **Scale**: Check-then-write races, unbounded in-memory growth, $O(N^2)$ loops, or N+1 queries.
4. **Missing Test**: Risky logic (branches, parsers, money, auth, state writes) lacking a falsifiable test that fails when broken.
5. **Speed**: Meaningful hot-path bottlenecks.
6. **Lean**: Dead code, duplicate helpers (name the existing symbol to reuse), stdlib-replaceable code, or single-use YAGNI wrappers.

## 3. 4-Part Output Format
Group numbered findings under **Must fix**, **Should fix**, and **Nice to have** (skip empty groups). Every finding MUST include all 4 parts in plain, direct language:
1. **`<Short Title>`** (`path/to/file.rs:Lstart-end`)
   - **What this is:** 1 sentence on what the code does.
   - **Problem:** Concrete input/scenario and what goes wrong.
   - **Fix:** Smallest code change that fixes it (prefer deleting code or reusing an existing symbol).
   - **If we skip it:** Concrete production or maintenance consequence.

End with:
- `Verdict: Ship.` or `Verdict: fix <numbers> first.`
- `Lean: -<N> lines possible.` when cuttable bloat or duplicate code exists.
- `Not checked: <one line>` if any runtime path could not be verified.
"#,
    },
    BuiltinSkillDef {
        name: "ponytail-audit",
        content: r#"---
name: "ponytail-audit"
description: "Whole-repo quality & lean audit (correctness, security, real load, missing tests, speed, and cuttable bloat: delete, reuse, stdlib, yagni, merge, split by job) with 4-part numbered findings and Lean: -<N> lines, -<M> dependencies possible."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["ponytail-audit", "repo audit", "lean audit", "bloat audit", "what to delete"]
allowed_tools: ["repo_map", "code_explore", "code_smells", "code_impact", "grep_search", "read_file"]
always_apply: false
---

# Ponytail Whole-Repo Lean & Quality Audit (`dietrichgebert/ponytail`)

Audit the repository like the senior engineer who just inherited it and will be paged when it breaks.

## 1. Map First
1. Run `repo_map(mode="reuse")` and `code_smells` to map entry points, PageRank-central shared modules, and structural smells.
2. Trace main data flows end-to-end (external input -> validation -> state/storage -> output).
3. Before calling any symbol unused, search the entire tree (`grep_search` + tests/configs) to confirm zero live references.
4. A shortcut marked with `// shortcut: <ceiling>, <when to upgrade>` is a recorded decision, NOT a defect, unless expected load already crosses its ceiling.

## 2. Lean Audit Categories
- **Delete**: Dead exports, unused config flags, speculative features.
- **Reuse**: Two helpers doing the same job (keep the PageRank-canonical one, name its path).
- **Stdlib / Native**: External dependencies or custom helpers duplicating standard library capabilities.
- **YAGNI**: Traits/interfaces with 1 implementation, factories with 1 product, pass-through wrappers.
- **Merge / Split by Job**: Near-copies that must change together, or god-functions mixing unrelated responsibilities (split by job, never by arbitrary line count).

## 3. Output Format
Start with `What this repo does:` (2–3 sentences + assumed load), followed by numbered 4-part findings (**Must fix** / **Should fix** / **Nice to have**: *What this is*, *Problem*, *Fix*, *If we skip it*, max 20 findings).
End with:
- `Verdict:` healthy, or which numbered items to fix first.
- `Lean: -<N> lines, -<M> dependencies possible.`
- `Not checked:` parts not read or executed.
"#,
    },
    BuiltinSkillDef {
        name: "ponytail-debt",
        content: r#"---
name: "ponytail-debt"
description: "Harvest every shortcut: <ceiling>, <when to upgrade> comment into a technical debt ledger via Tree-sitter AST (code_smells), separating triggered deferrals from [no-trigger] rot-risk items."
category: "quality"
kind: "workflow"
globs: ["**/*"]
triggers: ["ponytail-debt", "shortcut debt", "debt ledger", "technical debt ledger"]
allowed_tools: ["code_smells", "grep_search", "read_file"]
always_apply: false
---

# Ponytail Shortcut Technical Debt Ledger (`dietrichgebert/ponytail`)

Harvests deliberate `shortcut:` (and `ponytail:`) comments via Tree-sitter AST comment inspection so deferred simplifications never silently rot.

## 1. AST Scan Protocol
1. Run `code_smells` on the target path (or workspace root). `AstSmellDetector::harvest_shortcut_comments_ast` inspects real Tree-sitter comment AST nodes (`comment`, `line_comment`, `block_comment`), ignoring string literals and prose docs.
2. Each `shortcut: <ceiling>, <when to upgrade>` comment is classified into:
   - **Triggered (`INFO`, 0 health penalty)**: Names both a concrete limit (`ceiling`) and an explicit upgrade trigger (`when`, `once`, `if`, `after`, `above`, `past`, `beyond`, `upgrade`, `replace`, `switch`, `move`, `until`).
   - **Untriggered (`WARNING`, `[no-trigger]` rot risk)**: Missing a comma-separated upgrade condition or trigger keyword.

## 2. Ledger Output Format
Emit one row per marker, grouped by file:
`<file>:L<line> — ceiling: <limit>. upgrade: <trigger to revisit>.` (append `[no-trigger]` when no upgrade trigger is named).

End with:
`<N> shortcut markers, <M> with [no-trigger] rot risk.` (or `No shortcut debt. Clean ledger.` when zero markers exist).
"#,
    },
    BuiltinSkillDef {
        name: "ponytail-gain",
        content: r#"---
name: "ponytail-gain"
description: "Render the measured Ponytail impact scoreboard and live session quality telemetry (Reuse %, Open Callers, 4-state Test Freshness, Shortcut Debt Ledger) with strict honesty boundaries."
category: "efficiency"
kind: "workflow"
globs: ["**/*"]
triggers: ["ponytail-gain", "ponytail scoreboard", "reuse telemetry", "session quality"]
allowed_tools: ["code_smells", "optimize_token_budget", "power_diagnose"]
always_apply: false
---

# Ponytail Gain & Live Session Quality Scorecard (`dietrichgebert/ponytail`)

## 1. Live Session Quality Telemetry (`SessionQualityTelemetry`)
Report the real, measured figures from the current `minicode` session and repository:
- **Symbol Reuse Ratio (`Reuse %`)**: `reused_symbols / (reused_symbols + created_symbols)` tracked by Tree-sitter AST call/definition extraction across `write_file` and `patch_file` edits.
- **Open Callers (`open_callers`)**: Workspace callers (`file:line`) of removed or renamed symbols that still need updating (`0` required before completion).
- **4-State Test Freshness**: `NotRun` -> `Passed(N)` / `Failed(N)` -> `Stale(last_passed: N)` (automatically transitions to `Stale` whenever source code is modified after a test run).
- **Shortcut Debt Ledger**: Count of AST-verified `shortcut:` markers (`triggered` vs. `[no-trigger]` rot risk via `code_smells`).

## 2. Published Benchmark Reference & Honesty Boundary
```text
  ponytail benchmark reference · 39 tasks × 5 runs
  no-skill        ████████████████████  100%
  Lines of code   █████████···········   47%   ▼ 53%
  Output tokens   ███████████·········   55%   ▼ 45%
  Cost            ███████████████·····   74%   ▼ 26%
  Time            ████████████········   59%   ▼ 41%
  Hidden checks passed                 97%   (no-skill 96%)
  Tests where logic needs one          98%   (no-skill 68%)
```
- **Honesty Boundary**: Never invent a hypothetical "lines saved in this repo" number (the unbuilt version was never written). Report only real AST-counted telemetry (`Reuse %`, `open_callers`, `TestFreshnessState`, `shortcut:` ledger, and actual CCR token/USD compression stats).
"#,
    },
    BuiltinSkillDef {
        name: "design-md",
        content: r##"---
name: "design-md"
description: "Google Stitch & VoltAgent/awesome-design-md 9-Section DESIGN.md Design System Architecture — YAML design tokens (colors, typography, rounded, spacing, components), 84+ brand presets (Linear, Vercel, Stripe, Claude, Raycast, Supabase, Cursor, Warp, Resend, Mintlify), and UI generation invariants."
category: "ui-styling"
kind: "workflow"
globs: ["DESIGN.md", "design.md", "minikit_docs/core/design.md", "**/DESIGN.md"]
triggers: ["design-md", "awesome-design-md", "design.md", "design system", "design tokens", "voltagent", "stitch design", "brand design", "visual system", "getdesign.md"]
allowed_tools: ["read_file", "write_file", "patch_file", "vault_import_url", "vault_load", "block_palettes", "block_scaffold"]
always_apply: false
---

# DESIGN.md — Agent-First Design System Architecture (`VoltAgent/awesome-design-md` & Google Stitch)

## 1. The Three-Pillar Repository Documentation Contract
| File | Primary Reader | Authoritative Scope |
| :--- | :--- | :--- |
| **`README.md`** | Humans & contributors | What the project is, architecture anatomy, feature matrix, and zero-friction quickstart |
| **`.minicode/minicode.md` / `AGENTS.md`** | Coding agents | How to build, test, lint, and structure modules in the repository |
| **`DESIGN.md` / `minikit_docs/core/design.md`** | Design & UI agents | **How the product looks and feels** — exact hex tokens, typography hierarchy, elevation ladder, and component rules |

## 2. YAML Token Frontmatter Schema (Stitch + VoltAgent Format)
When creating or updating `DESIGN.md` (or `minikit_docs/core/design.md`), include structured YAML design tokens at the top of the file so `{colors.*}`, `{typography.*}`, and `{rounded.*}` references resolve deterministically:
```yaml
---
version: alpha
name: "<Brand or Project Name>"
description: "<1-sentence aesthetic, mood, and density summary>"
colors:
  primary: "#5e6ad2"
  on-primary: "#ffffff"
  primary-hover: "#828fff"
  canvas: "#08090a"
  surface-1: "#0f1011"
  surface-2: "#141516"
  surface-3: "#191a1b"
  ink: "#f7f8f8"
  ink-muted: "#d0d6e0"
  ink-subtle: "#8a8f98"
  hairline: "rgba(255, 255, 255, 0.08)"
  semantic-success: "#27a644"
  semantic-warning: "#f59e0b"
  semantic-error: "#ef4444"
typography:
  display-xl: { fontFamily: "Inter, system-ui, sans-serif", fontSize: "4.5rem", fontWeight: 510, lineHeight: 1.0, letterSpacing: "-0.022em" }
  headline:   { fontFamily: "Inter, system-ui, sans-serif", fontSize: "2rem",   fontWeight: 510, lineHeight: 1.13, letterSpacing: "-0.015em" }
  body:       { fontFamily: "Inter, system-ui, sans-serif", fontSize: "1rem",   fontWeight: 400, lineHeight: 1.6,  letterSpacing: "-0.011em" }
  mono:       { fontFamily: "JetBrains Mono, monospace",    fontSize: "0.875rem", fontWeight: 400, lineHeight: 1.5, letterSpacing: "0em" }
rounded:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  pill: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "24px"
  xl: "48px"
  section: "96px"
---
```

## 3. The 9 Canonical Sections of `DESIGN.md`
Every `DESIGN.md` MUST contain these 9 prose + table sections:
1. **`## 1. Visual Theme & Atmosphere`**: Overall mood, density, dark/light polarity, surface luminance ladder (`Level 0 Canvas` -> `Level 4 Glass`), and core design philosophy.
2. **`## 2. Color Palette & Roles`**: Semantic groups (`Brand & Accent`, `Surface`, `Text`, `Border & Divider`, `Semantic Status`) mapping every token (`{colors.primary}`) to its exact hex/rgba value and role.
3. **`## 3. Typography Rules`**: Primary/display/mono font families, open-source Google Fonts substitutes, and a **Hierarchy Table** (`| Role | Font | Size | Weight | Line Height | Letter Spacing | Usage |`) with OpenType features (`cv01`, `ss03`, `tabular-nums`).
4. **`## 4. Layout & Spacing Principles`**: Base spacing unit (`4px`/`8px`), 12-column grid, container max-widths (`1200px`/`1280px`), fixed-navbar hero clearance (`pt-28`+), and whitespace ratio.
5. **`## 5. Elevation & Depth`**: Markdown table of Levels `0..4` (`| Level | Name | Surface / Shadow Treatment | Use Case |`) plus `1px solid` hairline border-first containment and radial gradient glows.
6. **`## 6. Shapes & Border Radius Scale`**: Radius table (`xs` through `pill`/`full`) and media/card framing rules.
7. **`## 7. Component Stylings`**: Concrete specs for `Buttons` (Primary, Secondary, Ghost), `Cards & Containers`, `Inputs & Forms`, `Status Badges`, `Top Navigation`, and `Footer` across `default`, `hover`, `focus`, and `disabled` states.
8. **`## 8. Do's and Don'ts`**: `### Do` (5+ enforceable rules) and `### Don't` (5+ visual anti-patterns to reject).
9. **`## 9. Responsive Behavior & Agent Prompt Guide`**: Breakpoint table (`390px` mobile to `1440px` desktop), `44x44px` touch targets, and a copy-pasteable `### Iteration Checklist`.

## 4. Curated `VoltAgent/awesome-design-md` Brand Catalog & Instant Import
Import any of the 84+ real-world brand `DESIGN.md` systems directly into the project or vault via `vault_import_url(url="awesome-design-md/<slug>", scope="project")`:
| Brand Slug | Aesthetic Signature | Canvas / Primary / Ink | Typography |
| :--- | :--- | :--- | :--- |
| `linear.app` | Precision dark-mode craft, whisper-quiet hairlines | `#08090a` / `#5e6ad2` / `#f7f8f8` | `Inter Variable` (`w510`, `-0.022em`) + `Berkeley Mono` |
| `vercel` | Geist Swiss monochrome, 1px hairline grid architecture | `#000000` / `#ffffff` / `#ededed` | `Geist Sans` + `Geist Mono` |
| `stripe` | Atmospheric fintech gradients, layered glass elevation | `#0a2540` / `#635bff` / `#ffffff` | `Sohne` / `Inter` + `Source Code Pro` |
| `claude` | Warm editorial parchment, literary serif & terracotta | `#faf9f5` / `#d97757` / `#141413` | `Copernicus` / `Lora` + `Styrene` / `Inter` |
| `raycast` | High-density macOS command launcher, ruby-neon accents | `#07080a` / `#ff6363` / `#ffffff` | `Inter` + `JetBrains Mono` |
| `supabase` | Developer emerald dark mode, crisp Postgres telemetry | `#121212` / `#3ecf8e` / `#ededed` | `Circular` / `Inter` + `Source Code Pro` |
| `cursor` | AI-first IDE minimalism, warm dark carbon surfaces | `#0c0c0c` / `#f54e00` / `#edecec` | `Inter` + `JetBrains Mono` |
| `warp` | Modern GPU terminal aesthetic, kinetic dark surfaces | `#0c0d12` / `#01a4ff` / `#ffffff` | `Inter` + `Hack` / `JetBrains Mono` |
| `resend` | High-contrast dark developer marketing, specular borders | `#000000` / `#ffffff` / `#f8fafc` | `Plus Jakarta Sans` / `Inter` + `Geist Mono` |
| `mintlify` | Clean documentation hierarchy, emerald-teal clarity | `#090d16` / `#18e299` / `#f8fafc` | `Inter` + `JetBrains Mono` |
"##,
    },
    BuiltinSkillDef {
        name: "awesome-readme",
        content: r##"---
name: "awesome-readme"
description: "Craft flagship, high-conversion repository README.md files modeled on VoltAgent/awesome-design-md and top open-source projects — centered hero header with dynamic badges, 3-pillar contract tables, categorized feature matrices, ASCII/Mermaid architecture diagrams, and copy-pasteable quickstarts."
category: "docs"
kind: "workflow"
globs: ["README.md", "readme.md", "**/README.md"]
triggers: ["awesome-readme", "readme", "readme.md", "readme-architect", "make the readme", "update readme", "write readme", "improve readme", "github readme", "project readme"]
allowed_tools: ["read_file", "write_file", "patch_file", "grep_search", "locate_symbol"]
always_apply: false
---

# Awesome README.md Architecture Skill (`VoltAgent/awesome-design-md` Standard)

Whenever creating, updating, or improving a repository's `README.md`, never write a plain wall of text or generic placeholder bullets. Structure the `README.md` using this 8-section flagship open-source blueprint:

## 1. Centered Hero Header & Dynamic Shield Badges (`<div align="center">`)
Open with a centered HTML hero block that establishes visual authority in the first viewport:
```markdown
<div align="center">

<h1>project-name</h1>

<p>
  <strong>One-sentence high-signal value proposition explaining what it does and who it is for.</strong>
</p>

<p>
  <a href="#quickstart"><img src="https://img.shields.io/badge/Runtime-Stack_Version-61ffca?style=flat-square" alt="Runtime" /></a>
  <a href="DESIGN.md"><img src="https://img.shields.io/badge/Design_System-DESIGN.md-a277ff?style=flat-square" alt="DESIGN.md" /></a>
  <a href="#license"><img src="https://img.shields.io/badge/License-MIT-82e2ff?style=flat-square" alt="License" /></a>
</p>

</div>
```

## 2. "What It Is" + Three-Pillar Repository Contract Table
Immediately follow the hero with a crisp 2–3 sentence overview and a Markdown comparison table showing how human docs, agent rules, and design specs work together:
```markdown
| Artifact | Who Reads It | What It Defines |
| :--- | :--- | :--- |
| **[`README.md`](README.md)** | Humans & contributors | Project overview, architecture anatomy, feature catalog, and quickstart |
| **[`.minicode/minicode.md`](.minicode/minicode.md)** | Coding agents (`minicode`) | Build/test commands, module boundaries, and engineering invariants |
| **[`DESIGN.md`](DESIGN.md)** / **[`design.md`](minikit_docs/core/design.md)** | Design & UI agents | 9-section visual design system, color/typography tokens, and component rules |
```

## 3. Categorized Highlights & Capability Matrix
Group features into 3–5 themed subsections or a scannable table (`| Capability | Technical Mechanism | Impact |`) rather than an ungrouped 20-bullet list. Cite real modules, algorithms, and metrics verified from the codebase.

## 4. System Architecture & Numbered Module Anatomy Table
Include both:
1. A clean **ASCII or Mermaid (`flowchart TD` / `flowchart LR`)** diagram illustrating data flow and layer boundaries.
2. A **Numbered Anatomy Table** (`| # | Layer / Module | Path | Responsibility |`) mapping every real directory/entrypoint in the workspace.

## 5. Design System & Visual Craft Summary (`DESIGN.md` Link)
For web, mobile, TUI, or desktop projects, include a compact **Design Tokens & Aesthetic Table** (`Canvas`, `Primary Accent`, `Typography`, `Elevation`) referencing [`DESIGN.md`](DESIGN.md) or [`minikit_docs/core/design.md`](minikit_docs/core/design.md).

## 6. Zero-Friction Quickstart (Copy-Pasteable Commands)
Provide exact, verified commands extracted from `Cargo.toml`, `package.json`, `pyproject.toml`, or `Makefile`:
1. **Prerequisites & Install**
2. **Configuration (`.env` / CLI)**
3. **Run Development, Tests & Production Build**

## 7. CLI / API / Interactive Controls Reference Table
Include a Markdown table of CLI flags, API endpoints, or keyboard shortcuts (`| Command / Route | Arguments | Description |`).

## 8. Anti-Slop README Invariants
- **Ground in Live Code First**: Read `Cargo.toml` / `package.json` / `src/` before writing a single line of `README.md`. Never invent nonexistent flags, routes, or scripts.
- **Zero Placeholders**: Never leave `TODO`, `TBD`, `your-username/your-repo`, or `Add description here`.
- **Visual Rhythm**: Alternate prose paragraphs (max 3–4 lines) with Markdown tables, code blocks, and ASCII diagrams so the page is effortlessly scannable.
"##,
    },
    BuiltinSkillDef {
        name: "skillspector",
        content: r##"---
name: skillspector
description: Use when auditing agent skills (`SKILL.md` and bundled scripts), scanning third-party skills before installation, or investigating prompt injection, taint flows, Shannon entropy obfuscation, and least-privilege violations.
version: 1.0.0
author: minicode (inspired by NVIDIA/skillspector)
category: security
kind: workflow
triggers: [skillspector, scan-skill, skill-audit, skill-security, taint-analysis, tool-poisoning]
globs: ["**/SKILL.md", ".minicode/skills/**/*", "minikit_docs/skills/**/*"]
always_apply: false
allowed-tools: [vault_scan, vault_show, vault_search, read_file]
---

# SkillSpector — Algorithmic Static & Taint-Flow Security Auditor (`NVIDIA/skillspector`)

Every agent skill is an executable trust boundary combining natural-language instructions (`SKILL.md`) with bundled Level-3 scripts (`scripts/*.py`, `*.sh`, `*.js`, `*.ts`, `*.rs`). Use `vault_scan` to audit skills across 17 security categories before activating untrusted third-party skills.

## 1. Algorithmic Detection Engines
1. **Tree-sitter CST/AST & `petgraph` Dataflow Taint Reachability (`AST1`–`AST4`, `TT1`–`TT4`, `SBX1`)**:
   - Parses bundled Python, JavaScript, TypeScript, and Rust scripts into Concrete Syntax Trees.
   - Resolves module import aliases and constructs an intraprocedural `DiGraph` linking taint sources (`os.environ`, `process.env`, credential files, network inputs) through variable bindings to execution/network sinks (`eval`, `exec`, `subprocess`, `requests.post`, `fetch`).
   - Uses `petgraph::algo::astar` graph reachability to prove source-to-sink exfiltration and remote network-to-exec chains.
2. **Information-Theoretic Shannon Entropy & Binary Magic Classifier (`SC3`, `SC8`, `SC9`, `CRED1`, `P9`)**:
   - Computes exact Shannon entropy $H(X) = -\sum P(x_i) \log_2 P(x_i)$ over token byte streams to detect Base64/encrypted payloads ($H \ge 5.15$ bits/byte) and hardcoded live credentials ($H \ge 4.10$ bits/byte).
   - Inspects file headers against native binary/archive magic tables (`ELF`, `Mach-O`, `PE`, `Wasm`, `.pyc`, `ZIP`, `GZIP`) and horizontal whitespace run-length padding ($\ge 64$ columns).
3. **Unicode Script-Partition & Stealth Automaton (`TP1`, `TP2`)**:
   - Detects invisible zero-width joiners, Bidirectional text overrides (`U+202A..U+202E`), and Unicode Language Tag block smuggling (`U+E0000..U+E007F`).
   - Computes per-identifier script bitmasks (`Latin | Cyrillic | Greek`) to flag homoglyph tool poisoning.
4. **`pulldown-cmark` Markdown AST, Least-Privilege Set Algebra & Intent Vectors (`P1`, `P2`, `P6`, `AR1`, `MP1`, `EA1`, `LP1`–`LP3`, `TR1`–`TR3`)**:
   - Computes set difference $\Delta = \text{ExercisedCapabilities} \setminus \text{DeclaredCapabilities}$ against `frontmatter.allowed_tools`.
   - Evaluates sliding-window stem co-occurrence vectors and SSRF cloud metadata URLs (`169.254.169.254`).

## 2. Deterministic Risk Score & Pre-Install Gate
- **Formula**: $\text{score} = \min(100, \lfloor (\sum w(\text{severity})) \times M_{\text{script}} \rfloor)$, where $w(\text{Critical})=50, w(\text{High})=25, w(\text{Medium})=10, w(\text{Low})=5$, and $M_{\text{script}} = 1.3$ when executable scripts are bundled.
- **Verdicts**:
  - `0..=20` (`LOW` / `SAFE`): Safe to install.
  - `21..=50` (`MEDIUM` / `CAUTION`): Review findings before use.
  - `51..=100` (`HIGH`..`CRITICAL` / `DO_NOT_INSTALL`): Automatically blocked by `VaultStore` during `vault_import_url`, `vault_ingest_source`, and `vault_load`.

## 3. Workflow & Tool Usage
- **Audit any vault skill, local directory, or remote URL**:
  `vault_scan(target="skill-name-or-path-or-url", format="markdown" | "json" | "sarif")`
- **Baseline Suppression**:
  Pass `suppress_fingerprints=["sp-..."]` to `vault_scan` after manually verifying a benign finding to record its FNV-1a evidence fingerprint in `.minicode/vault/skillspector_baseline.json`.
"##,
    },
    BuiltinSkillDef {
        name: "speckit",
        content: r#"---
name: "speckit"
description: "GitHub Spec-Kit (github/spec-kit) Specification-Driven Development (SDD) engine: Bipartite petgraph traceability analyzer (analyze), 5-dimension Requirements-Quality Checklist ('Unit Tests for Requirements'), Append-Only Convergence loop (converge), 3-stage Scoped Bugfix workflow (assess -> fix -> test), and 5-stage Evidence-Weighted Idea Assessment scorecard."
category: "quality"
kind: "workflow"
globs: ["minikit_docs/core/*.md", "onpkg_docs/core/*.md", ".minicode/sdd/**/*.md", ".minicode/plan/task_plan.md"]
triggers: ["speckit", "spec-kit", "sdd", "spec driven", "traceability", "requirements checklist", "converge", "bug triage", "idea assessment"]
allowed_tools: ["power_speckit", "power_brainstorm", "power_plan", "power_verify", "ask_user", "record_decision", "read_file", "write_file"]
always_apply: false
---

# SpecKit Specification-Driven Development (`github/spec-kit`)

## 5 Native Algorithmic SDD Capabilities (`power_speckit`)
1. **Bipartite Cross-Artifact Traceability & Consistency Analyzer (`action="analyze"`)**:
   - Parses `prd.md`, `spec.md`, `architecture.md`, `implementation.md`, and `todo.md` / `task_plan.md` via `pulldown-cmark` AST events.
   - Builds an IDF-weighted `petgraph::DiGraph` linking `Requirement -> PlanComponent -> Task` via morphological stems and target deliverable files.
   - Detects **unmapped requirements** (zero path to any task), **orphan tasks**, and unresolved `[NEEDS CLARIFICATION]` / `[ASSUMPTION]` markers.
2. **Requirements-Quality Checklist ("Unit Tests for Requirements", `action="checklist"`)**:
   - Evaluates 5 deterministic requirement dimensions (`RQ-1` Zero Ambiguity, `RQ-2` Testable Acceptance Criteria, `RQ-3` Bounded Non-Goals, `RQ-4` Architecture Alignment, `RQ-5` Bipartite Task Coverage) and persists `core/checklists/requirements.md`.
3. **Append-Only Implementation Convergence Engine (`action="converge"`)**:
   - Audits the live workspace against `spec.md` and `todo.md`.
   - Resolves strictly to `CONVERGED` (leaving `todo.md` byte-for-byte untouched) or `TasksAppended` (appending missing delta tasks under `## Convergence` without overwriting or deleting existing tasks).
4. **Scoped 3-Stage Bug Triage State Machine (`action="bug"`, `stage="assess" | "fix" | "test"`)**:
   - Persists `.minicode/sdd/bugs/<slug>/{assessment,fix,test}.md`.
   - Computes set difference `ModifiedFiles \ AssessedFiles` under `## Deviations from Assessment` and strictly downgrades verification to `PARTIAL` if the original reproduction was not exercised.
5. **5-Stage Evidence-Weighted Idea Assessment (`action="assess"`, `stage="intake" | "research" | "define" | "shape" | "decide"`)**:
   - Evaluates problem validity, supporting/counter-evidence, concept viability, and `[ASSUMPTION]` markers in `.minicode/sdd/assessments/<slug>/` to emit `GO`, `NEEDS_CLARIFICATION`, or `KILL`.
"#,
    },
];

/// Returns all built-in skills as parsed `VaultSkill` items.
pub fn get_all_builtin_skills() -> Vec<VaultSkill> {
    BUILTIN_SKILLS
        .iter()
        .map(|def| {
            let (fm, instructions) = parse_skill_markdown(def.content, def.name);
            VaultSkill {
                name: fm.name.clone(),
                description: fm.description.clone(),
                scope: SkillScope::Builtin,
                frontmatter: fm,
                instructions,
                raw_content: def.content.to_string(),
                path: None,
                is_active_in_project: false,
            }
        })
        .collect()
}

/// Finds a built-in skill by name or trigger/alias.
pub fn find_builtin_skill(name: &str) -> Option<VaultSkill> {
    let clean = name.trim().to_lowercase();
    let all = get_all_builtin_skills();
    // 1. Exact name match
    if let Some(s) = all.iter().find(|s| s.name.eq_ignore_ascii_case(&clean)) {
        return Some(s.clone());
    }
    // 2. Trigger or prefix alias match (e.g. "tailwind" -> "tailwind-v4", "next" -> "nextjs", "rust" -> "rust-tokio")
    if let Some(s) = all.iter().find(|s| {
        s.frontmatter
            .triggers
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&clean))
            || s.name.starts_with(&format!("{}-", clean))
    }) {
        return Some(s.clone());
    }
    None
}

/// Definition of a built-in curated skill bundle.
pub struct BuiltinBundleDef {
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub skills: &'static [&'static str],
    pub tags: &'static [&'static str],
}

pub const BUILTIN_BUNDLES: &[BuiltinBundleDef] = &[
    BuiltinBundleDef {
        name: "fullstack-nextjs",
        description: "Fullstack Next.js App Router with React 19, Tailwind CSS v4, PostgreSQL, and high-end frontend design.",
        category: "fullstack",
        skills: &["react", "nextjs", "tailwind-v4", "postgres", "frontend-design"],
        tags: &["nextjs", "react", "tailwind", "postgres", "fullstack"],
    },
    BuiltinBundleDef {
        name: "rust-systems",
        description: "High-performance async backend services with Tokio, PostgreSQL, Docker, and TDD workflow.",
        category: "backend",
        skills: &["rust-tokio", "postgres", "docker", "tdd-workflow"],
        tags: &["rust", "tokio", "systems", "backend", "tdd"],
    },
    BuiltinBundleDef {
        name: "fastapi-microservice",
        description: "Python async REST API microservices with FastAPI, Pydantic v2, PostgreSQL, Docker, and TDD.",
        category: "backend",
        skills: &["fastapi", "postgres", "docker", "tdd-workflow"],
        tags: &["python", "fastapi", "backend", "docker"],
    },
    BuiltinBundleDef {
        name: "frontend-delight",
        description: "Award-winning landing pages, interactive animations, and design tokens with GSAP and Tailwind.",
        category: "frontend",
        skills: &["frontend-design", "ui-ux-pro-max", "tailwind-v4", "gsap"],
        tags: &["design", "animation", "gsap", "tailwind", "landing-page"],
    },
    BuiltinBundleDef {
        name: "production-hardened",
        description: "Enterprise engineering rigor: security audits, SkillSpector static/taint skill scanning, systematic debugging, TDD workflow, and Docker packaging.",
        category: "quality",
        skills: &[
            "security-audit",
            "skillspector",
            "systematic-debugging",
            "tdd-workflow",
            "docker",
        ],
        tags: &["security", "skillspector", "debugging", "tdd", "production"],
    },
    BuiltinBundleDef {
        name: "modern-fullstack",
        description: "Modern edge-ready fullstack web apps with React, Vite, Hono API, Tailwind v4, and PostgreSQL.",
        category: "fullstack",
        skills: &["react", "hono", "vite", "tailwind-v4", "postgres"],
        tags: &["react", "hono", "vite", "edge", "fullstack"],
    },
    BuiltinBundleDef {
        name: "anthropic-document-suite",
        description: "Official Anthropic Document & Office Suite: Word (.docx), PDF, PowerPoint (.pptx), Excel (.xlsx), Technical Doc Co-Authoring, and Internal Comms.",
        category: "docs",
        skills: &["docx", "pdf", "pptx", "xlsx", "doc-coauthoring", "internal-comms"],
        tags: &["anthropic", "docx", "pdf", "pptx", "xlsx", "documents", "office"],
    },
    BuiltinBundleDef {
        name: "anthropic-creative-studio",
        description: "Official Anthropic Design & Creative Studio: Frontend Design, Canvas Posters, Algorithmic p5.js Art, Theme Factory, Web Artifacts Builder, Brand Guidelines, and Slack GIFs.",
        category: "frontend",
        skills: &[
            "frontend-design",
            "canvas-design",
            "algorithmic-art",
            "theme-factory",
            "web-artifacts-builder",
            "brand-guidelines",
            "slack-gif-creator",
        ],
        tags: &["anthropic", "design", "canvas", "generative-art", "theme", "artifacts", "gif"],
    },
    BuiltinBundleDef {
        name: "anthropic-agent-engineering",
        description: "Official Anthropic Agent & Tool Engineering Pack: MCP Server Builder, Playwright Webapp E2E Testing, Skill Creator, and Claude API SDK patterns.",
        category: "backend",
        skills: &["mcp-builder", "webapp-testing", "skill-creator", "claude-api"],
        tags: &["anthropic", "mcp", "playwright", "skill-creator", "claude-api", "agents"],
    },
    BuiltinBundleDef {
        name: "minipower-superpowers-suite",
        description: "Complete MiniPower / obra/superpowers v7.0.1 Workflow Suite: Brainstorming, Writing Plans, Inline & Subagent Execution, Parallel Fanout, Falsifiable TDD, Systematic Debugging, Two-Stage Code Review, Git Worktrees, Verification Gates, Skill TDD, and Session Forensics.",
        category: "quality",
        skills: &[
            "using-minipower",
            "brainstorming",
            "writing-plans",
            "executing-plans",
            "subagent-driven-development",
            "dispatching-parallel-agents",
            "tdd-workflow",
            "systematic-debugging",
            "verification-before-completion",
            "requesting-code-review",
            "receiving-code-review",
            "using-git-worktrees",
            "finishing-a-development-branch",
            "writing-skills",
            "diagnosing-minipower",
            "speckit",
        ],
        tags: &[
            "minipower",
            "superpowers",
            "workflow",
            "tdd",
            "subagents",
            "worktree",
            "review",
            "diagnostics",
        ],
    },
    BuiltinBundleDef {
        name: "minicode-orchestrator-suite",
        description: "Complete MiniCode Orchestrator & Subsystem Capability Suite: Socratic Ask-User Inquiry, Two-Tier Todo & Plan Orchestration, MiniBlocks UI Warehouse, MiniPubs 14-Role Delegation, Reactive Event-Driven Async DAG Orchestration, MiniTask Background Runtime & Browser QA, MiniKit Stack Scaffolding, and Structural AST/CodeGraph Navigation.",
        category: "orchestrator",
        skills: &[
            "using-minipower",
            "ask-user-inquiry",
            "todo-planning",
            "miniblocks-ui",
            "minipubs-delegation",
            "async-dag-orchestration",
            "minitask-runtime",
            "minikit-scaffolding",
            "ast-codegraph-navigation",
            "brainstorming",
            "writing-plans",
            "executing-plans",
            "subagent-driven-development",
            "dispatching-parallel-agents",
            "verification-before-completion",
        ],
        tags: &[
            "orchestrator",
            "minicode",
            "ask_user",
            "todo",
            "miniblocks",
            "minipubs",
            "async-dag",
            "minitask",
            "minikit",
            "ast-grep",
            "codegraph",
        ],
    },
    BuiltinBundleDef {
        name: "caveman-efficiency-suite",
        description: "Complete juliusbrussee/caveman v4.0 ('Six Rocks') Token Efficiency & Tactical Execution Suite: Caveman 9-Rule ASD-STE100 Voice, UltraCave, MegaCave (Extreme English Telegraphic Mode), CaveCrew Subagents, FastContext Evidence Scout, Caveman Commit/Review/Compress/Learn, and the 6 Native Core Tactical Skills (investigate-first, lean-build, safe-refactor, surgical-patch, verify-and-stop, migration).",
        category: "efficiency",
        skills: &[
            "caveman",
            "ultracave",
            "megacave",
            "cavecrew",
            "caveman-explore",
            "caveman-commit",
            "caveman-review",
            "caveman-compress",
            "caveman-learn",
            "investigate-first",
            "lean-build",
            "safe-refactor",
            "surgical-patch",
            "verify-and-stop",
            "migration",
        ],
        tags: &[
            "caveman",
            "ultracave",
            "megacave",
            "telegraphic",
            "cavecrew",
            "fastcontext",
            "token-efficiency",
            "native-core",
        ],
    },
    BuiltinBundleDef {
        name: "ponytail-minimalist-suite",
        description: "Complete dietrichgebert/ponytail Minimalist Engineering & YAGNI Quality Suite: 7-Rung Solution Ladder & Symbol Reuse (ponytail), 4-Part Plain-Language Diff Review + Lean Line Delta (ponytail-review), Whole-Repo Lean & Bloat Audit (ponytail-audit), Tree-Sitter AST Shortcut Debt Ledger (ponytail-debt), and Live Session Reuse/Freshness Scorecard (ponytail-gain).",
        category: "efficiency",
        skills: &[
            "ponytail",
            "ponytail-review",
            "ponytail-audit",
            "ponytail-debt",
            "ponytail-gain",
        ],
        tags: &[
            "ponytail",
            "yagni",
            "minimalist",
            "reuse",
            "shortcut-debt",
            "lean-review",
            "lean-audit",
        ],
    },
    BuiltinBundleDef {
        name: "awesome-design-and-docs-suite",
        description: "VoltAgent/awesome-design-md & Flagship Repository Documentation Suite: 9-Section Stitch/VoltAgent DESIGN.md Architecture & 84+ Brand Preset Catalog (design-md), High-Conversion Open-Source README.md Architect (awesome-readme), Frontend Design, UI/UX Pro Max, Theme Factory, and Technical Doc Co-Authoring.",
        category: "docs",
        skills: &[
            "design-md",
            "awesome-readme",
            "frontend-design",
            "ui-ux-pro-max",
            "theme-factory",
            "brand-guidelines",
            "doc-coauthoring",
        ],
        tags: &[
            "design-md",
            "awesome-design-md",
            "readme",
            "awesome-readme",
            "voltagent",
            "stitch",
            "design-system",
            "documentation",
        ],
    },
];

/// Returns all built-in bundles as `VaultBundle` items.
pub fn get_all_builtin_bundles() -> Vec<VaultBundle> {
    BUILTIN_BUNDLES
        .iter()
        .map(|def| VaultBundle {
            name: def.name.to_string(),
            description: def.description.to_string(),
            category: def.category.to_string(),
            skills: def.skills.iter().map(|s| s.to_string()).collect(),
            tags: def.tags.iter().map(|t| t.to_string()).collect(),
            scope: SkillScope::Builtin,
            path: None,
        })
        .collect()
}

/// Finds a built-in bundle by name.
pub fn find_builtin_bundle(name: &str) -> Option<VaultBundle> {
    let clean = name.trim().to_lowercase();
    get_all_builtin_bundles()
        .into_iter()
        .find(|b| b.name.eq_ignore_ascii_case(&clean))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_catalog_integrity() {
        let skills = get_all_builtin_skills();
        assert!(skills.len() >= 15);

        for skill in &skills {
            assert!(!skill.name.is_empty());
            assert!(!skill.description.is_empty());
            assert!(!skill.instructions.is_empty());
            assert_eq!(skill.scope, SkillScope::Builtin);
        }

        let tailwind = find_builtin_skill("tailwind-v4");
        assert!(tailwind.is_some());
        let tw = tailwind.unwrap();
        assert!(tw.instructions.contains("@import \"tailwindcss\""));
        assert_eq!(tw.category(), "ui-styling");
        assert_eq!(tw.kind(), crate::vault::models::SkillKind::Reference);

        // Verify kind distinctions: PackageDoc vs Behavioral Skill vs Runtime Workflow
        let react = find_builtin_skill("react").unwrap();
        assert_eq!(react.kind(), crate::vault::models::SkillKind::Reference);
        assert_eq!(react.kind().badge(), "Doc");

        let tdd = find_builtin_skill("tdd-workflow").unwrap();
        assert_eq!(tdd.kind(), crate::vault::models::SkillKind::Workflow);
        assert_eq!(tdd.kind().badge(), "Skill");

        let rust = find_builtin_skill("rust-tokio").unwrap();
        assert_eq!(rust.kind(), crate::vault::models::SkillKind::Runtime);
        assert_eq!(rust.kind().badge(), "Runtime");
    }

    #[test]
    fn test_builtin_bundles_integrity() {
        let bundles = get_all_builtin_bundles();
        assert!(bundles.len() >= 6);

        for bundle in &bundles {
            assert!(!bundle.name.is_empty());
            assert!(!bundle.description.is_empty());
            assert!(!bundle.skills.is_empty());
            assert_eq!(bundle.scope, SkillScope::Builtin);

            // Verify all skills in bundle exist in built-in skills
            for skill_name in &bundle.skills {
                assert!(
                    find_builtin_skill(skill_name).is_some(),
                    "Bundle '{}' references non-existent skill '{}'",
                    bundle.name,
                    skill_name
                );
            }
        }

        let nextjs_bundle = find_builtin_bundle("fullstack-nextjs");
        assert!(nextjs_bundle.is_some());
        let nb = nextjs_bundle.unwrap();
        assert!(nb.skills.contains(&"react".to_string()));
        assert!(nb.skills.contains(&"nextjs".to_string()));

        // Verify all 17 official anthropics/skills are present in MiniVault and MiniKit
        for anthropic_skill in [
            "frontend-design",
            "mcp-builder",
            "webapp-testing",
            "skill-creator",
            "doc-coauthoring",
            "web-artifacts-builder",
            "canvas-design",
            "algorithmic-art",
            "theme-factory",
            "slack-gif-creator",
            "claude-api",
            "docx",
            "pdf",
            "pptx",
            "xlsx",
            "brand-guidelines",
            "internal-comms",
        ] {
            assert!(
                find_builtin_skill(anthropic_skill).is_some(),
                "Missing anthropics/skills entry in MiniVault: {}",
                anthropic_skill
            );
            assert!(
                crate::tools::minikit::builtin_skills::find_builtin_skill(anthropic_skill)
                    .is_some(),
                "Missing anthropics/skills entry in MiniKit: {}",
                anthropic_skill
            );
        }

        assert!(find_builtin_bundle("anthropic-document-suite").is_some());
        assert!(find_builtin_bundle("anthropic-creative-studio").is_some());
        assert!(find_builtin_bundle("anthropic-agent-engineering").is_some());
        assert!(find_builtin_bundle("minipower-superpowers-suite").is_some());
        assert!(find_builtin_bundle("minicode-orchestrator-suite").is_some());

        // Verify all 8 MiniCode Orchestrator & Subsystem skills are present in MiniVault and MiniKit
        for orchestrator_skill in [
            "ask-user-inquiry",
            "todo-planning",
            "miniblocks-ui",
            "minipubs-delegation",
            "async-dag-orchestration",
            "minitask-runtime",
            "minikit-scaffolding",
            "ast-codegraph-navigation",
        ] {
            assert!(
                find_builtin_skill(orchestrator_skill).is_some(),
                "Missing MiniCode orchestrator skill in MiniVault: {}",
                orchestrator_skill
            );
            assert!(
                crate::tools::minikit::builtin_skills::find_builtin_skill(orchestrator_skill)
                    .is_some(),
                "Missing MiniCode orchestrator skill in MiniKit: {}",
                orchestrator_skill
            );
        }

        // Verify all 15 obra/superpowers v7.0.1 skills (plus upstream aliases) are present in MiniVault and MiniKit
        for minipower_skill in [
            "using-minipower",
            "using-superpowers",
            "brainstorming",
            "writing-plans",
            "executing-plans",
            "subagent-driven-development",
            "dispatching-parallel-agents",
            "tdd-workflow",
            "test-driven-development",
            "systematic-debugging",
            "verification-before-completion",
            "requesting-code-review",
            "receiving-code-review",
            "using-git-worktrees",
            "finishing-a-development-branch",
            "writing-skills",
            "diagnosing-minipower",
            "diagnosing-superpowers",
        ] {
            let vault_skill = find_builtin_skill(minipower_skill);
            assert!(
                vault_skill.is_some(),
                "Missing MiniPower/superpowers skill in MiniVault: {}",
                minipower_skill
            );
            assert!(
                crate::tools::minikit::builtin_skills::find_builtin_skill(minipower_skill)
                    .is_some(),
                "Missing MiniPower/superpowers skill in MiniKit: {}",
                minipower_skill
            );
        }

        // Verify all 15 juliusbrussee/caveman v4.0 skills + caveman-efficiency-suite bundle are present in MiniVault and MiniKit
        assert!(find_builtin_bundle("caveman-efficiency-suite").is_some());
        for caveman_skill in [
            "caveman",
            "ultracave",
            "megacave",
            "cavecrew",
            "caveman-explore",
            "caveman-commit",
            "caveman-review",
            "caveman-compress",
            "caveman-learn",
            "investigate-first",
            "lean-build",
            "safe-refactor",
            "surgical-patch",
            "verify-and-stop",
            "migration",
        ] {
            assert!(
                find_builtin_skill(caveman_skill).is_some(),
                "Missing juliusbrussee/caveman skill in MiniVault: {}",
                caveman_skill
            );
            assert!(
                crate::tools::minikit::builtin_skills::find_builtin_skill(caveman_skill).is_some(),
                "Missing juliusbrussee/caveman skill in MiniKit: {}",
                caveman_skill
            );
        }

        // Verify all 5 dietrichgebert/ponytail skills + ponytail-minimalist-suite bundle are present in MiniVault and MiniKit
        assert!(find_builtin_bundle("ponytail-minimalist-suite").is_some());
        for ponytail_skill in [
            "ponytail",
            "ponytail-review",
            "ponytail-audit",
            "ponytail-debt",
            "ponytail-gain",
        ] {
            assert!(
                find_builtin_skill(ponytail_skill).is_some(),
                "Missing dietrichgebert/ponytail skill in MiniVault: {}",
                ponytail_skill
            );
            assert!(
                crate::tools::minikit::builtin_skills::find_builtin_skill(ponytail_skill).is_some(),
                "Missing dietrichgebert/ponytail skill in MiniKit: {}",
                ponytail_skill
            );
        }

        // Verify VoltAgent/awesome-design-md (`design-md`), `awesome-readme`, `skillspector`, and `speckit` skills
        assert!(find_builtin_bundle("awesome-design-and-docs-suite").is_some());
        for design_doc_skill in ["design-md", "awesome-readme", "skillspector", "speckit"] {
            assert!(
                find_builtin_skill(design_doc_skill).is_some(),
                "Missing design/readme skill in MiniVault: {}",
                design_doc_skill
            );
            assert!(
                crate::tools::minikit::builtin_skills::find_builtin_skill(design_doc_skill)
                    .is_some(),
                "Missing design/readme skill in MiniKit: {}",
                design_doc_skill
            );
        }
    }
}
