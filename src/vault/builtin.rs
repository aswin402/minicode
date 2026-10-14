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
description: "Async Rust, Tokio multi-threaded runtime, channels, bounded tasks, and zero-panic error handling."
category: "runtime"
kind: "runtime"
globs: ["*.rs", "Cargo.toml"]
triggers: ["rust", "tokio", "async rust", "cargo", "channel"]
always_apply: false
---

# Async Rust & Tokio Standards

## Invariants
1. **Zero Panics**: Never use `.unwrap()` or `.expect()` in non-test production code. Return `Result<T, E>` with `thiserror` for library code and `anyhow` at CLI boundaries.
2. **Cancellation Safety**: Ensure all `tokio::select!` branches are cancellation safe. Do not drop non-reentrant state across awaits.
3. **Bounded Channels**: Always prefer bounded channels (`tokio::sync::mpsc::channel(N)`) over unbounded channels to prevent memory leaks under backpressure.
4. **Blocking Work**: Offload CPU-heavy or blocking filesystem operations to `tokio::task::spawn_blocking`.
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
    }
}
