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
triggers: ["nextjs", "next.js", "app router", "server actions", "rsc"]
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
description: "Test-Driven Development (TDD) workflow: red, green, refactor, and high-coverage assertions."
category: "testing"
kind: "workflow"
globs: ["tests/**/*", "*_test.*", "*.spec.*", "*.test.*"]
triggers: ["tdd", "test", "unit test", "integration test"]
always_apply: false
---

# Test-Driven Development (TDD) Workflow

## Invariants
1. **Red First**: Write a focused, failing unit test before implementing the solution code.
2. **Minimal Green**: Write the minimal implementation code necessary to make the failing test pass.
3. **Refactor Cleanly**: Refactor the implementation for clarity and performance while keeping tests passing.
4. **Boundary Testing**: Test happy path, empty collections, invalid inputs, and error/exception branches.
"#,
    },
    BuiltinSkillDef {
        name: "systematic-debugging",
        content: r#"---
name: "systematic-debugging"
description: "4-phase systematic debugging: reproduce, localize root cause, implement minimal fix, and verify."
category: "testing"
kind: "workflow"
globs: ["**/*"]
triggers: ["bug", "error", "debug", "crash", "fix", "failure"]
always_apply: false
---

# Systematic Debugging Protocol

## 4-Phase Protocol
1. **Reproduce**: Capture exact error logs, stack traces, and deterministic reproduction steps.
2. **Trace Root Cause**: Formulate a hypothesis and verify it using logs or tests. Do not guess or apply random trial-and-error changes.
3. **Minimal Fix**: Address the underlying structural defect without introducing side-effects or breaking existing invariants.
4. **Verify & Guard**: Run test suites to verify the fix and write a regression test to prevent recurrence.
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
globs: ["*.tsx", "*.jsx", "*.js", "*.ts", "*.html"]
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
allowed_tools: ["exec_cmd", "read_file", "write_file", "edit_file"]
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
allowed_tools: ["vault_create", "vault_update", "vault_show", "vault_search", "write_file", "edit_file"]
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
allowed_tools: ["read_file", "write_file", "edit_file"]
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
   - Apply surgical edits (`edit_file`) based on user feedback.

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
allowed_tools: ["exec_cmd", "write_file", "edit_file", "browser_navigate", "browser_screenshot"]
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
globs: ["*.svg", "*.pdf", "posters/**/*", "canvas/**/*"]
triggers: ["canvas-design", "poster design", "visual art", "svg poster", "graphic design", "print pdf"]
allowed_tools: ["exec_cmd", "write_file", "edit_file"]
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
allowed_tools: ["write_file", "edit_file", "browser_navigate", "browser_screenshot"]
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
allowed_tools: ["exec_cmd", "read_file", "write_file", "edit_file"]
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
allowed_tools: ["exec_cmd", "read_file", "write_file", "edit_file"]
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
allowed_tools: ["exec_cmd", "read_file", "write_file", "edit_file"]
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
allowed_tools: ["exec_cmd", "read_file", "write_file", "edit_file"]
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
allowed_tools: ["read_file", "write_file", "edit_file"]
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
        description: "Enterprise engineering rigor: security audits, systematic debugging, TDD workflow, and Docker packaging.",
        category: "quality",
        skills: &["security-audit", "systematic-debugging", "tdd-workflow", "docker"],
        tags: &["security", "debugging", "tdd", "production"],
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
    }
}
