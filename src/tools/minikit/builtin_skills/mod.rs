//! Embedded library of 14 battle-tested, production domain skills in MiniKit.

pub struct BuiltinSkill {
    pub name: &'static str,
    pub description: &'static str,
    pub globs: &'static [&'static str],
    pub content: &'static str,
}

pub fn get_all_builtin_skills() -> &'static [BuiltinSkill] {
    static SKILLS: &[BuiltinSkill] = &[
        BuiltinSkill {
            name: "react",
            description: "AI Agent Skill for React 19 — hooks, component lifecycle, styling, state management, and performance",
            globs: &["**/*.tsx", "**/*.jsx", "**/react*.js"],
            content: include_str!("react.md"),
        },
        BuiltinSkill {
            name: "next",
            description: "AI Agent Skill for Next.js 16 — App Router, Server Components, Server Actions, route handlers, and SSR",
            globs: &["**/app/**/*.ts", "**/app/**/*.tsx", "**/pages/**/*.tsx", "**/next.config.*"],
            content: include_str!("next.md"),
        },
        BuiltinSkill {
            name: "fastapi",
            description: "AI Agent Skill for FastAPI — Pydantic v2, async SQLAlchemy, dependency injection, and OpenAPI",
            globs: &["**/*.py", "**/requirements*.txt", "**/pyproject.toml"],
            content: include_str!("fastapi.md"),
        },
        BuiltinSkill {
            name: "flutter",
            description: "AI Agent Skill for Flutter — Riverpod/HooksRiverpod, GoRouter, Material 3, Dio, and responsive layout",
            globs: &["**/*.dart", "**/pubspec.yaml"],
            content: include_str!("flutter.md"),
        },
        BuiltinSkill {
            name: "hono",
            description: "AI Agent Skill for Hono API — Bun/Cloudflare edge runtime, middleware, routing, and Pino logging",
            globs: &["**/routes/**/*.ts", "**/src/index.ts", "**/hono*.ts"],
            content: include_str!("hono.md"),
        },
        BuiltinSkill {
            name: "rust",
            description: "AI Agent Skill for Rust — Tokio async, thiserror/anyhow, ownership, safety, and idiomatic concurrency",
            globs: &["**/*.rs", "**/Cargo.toml"],
            content: include_str!("rust.md"),
        },
        BuiltinSkill {
            name: "tailwind",
            description: "AI Agent Skill for Tailwind CSS v4 — container queries, CSS-first config, dark mode, and design tokens",
            globs: &["**/*.css", "**/tailwind.config.*", "**/*.postcss"],
            content: include_str!("tailwind.md"),
        },
        BuiltinSkill {
            name: "mongodb",
            description: "AI Agent Skill for MongoDB — Mongoose schemas, compound indexes, aggregation pipelines, and ACID transactions",
            globs: &["**/models/**/*.ts", "**/models/**/*.js", "**/mongoose*.ts"],
            content: include_str!("mongodb.md"),
        },
        BuiltinSkill {
            name: "postgres",
            description: "AI Agent Skill for PostgreSQL — connection pooling, transactions, JSONB, migrations, and performance indexing",
            globs: &["**/*.sql", "**/migrations/**/*.sql", "**/db/**/*.ts"],
            content: include_str!("postgres.md"),
        },
        BuiltinSkill {
            name: "prisma",
            description: "AI Agent Skill for Prisma ORM — schema modeling, relations, migrations, Prisma client queries, and connection pools",
            globs: &["**/schema.prisma", "**/prisma/**/*.ts"],
            content: include_str!("prisma.md"),
        },
        BuiltinSkill {
            name: "vite",
            description: "AI Agent Skill for Vite 8 — lightning-fast HMR, plugin ecosystem, bundle optimization, and static assets",
            globs: &["**/vite.config.*"],
            content: include_str!("vite.md"),
        },
        BuiltinSkill {
            name: "express",
            description: "AI Agent Skill for Express.js — TypeScript, modular routers, security middleware, and REST architecture",
            globs: &["**/routes/**/*.js", "**/routes/**/*.ts", "**/server.ts", "**/server.js"],
            content: include_str!("express.md"),
        },
        BuiltinSkill {
            name: "frontend-design",
            description: "Production-grade UI design skill — distinctive aesthetics, modern layouts, micro-interactions, and accessibility",
            globs: &["**/*.tsx", "**/*.jsx", "**/*.html", "**/*.css", "**/*.vue", "**/*.svelte"],
            content: include_str!("frontend-design/SKILL.md"),
        },
        BuiltinSkill {
            name: "ui-ux-pro-max",
            description: "Comprehensive UI/UX design masterclass — color psychology, typography systems, mobile responsive, and layout grids",
            globs: &["**/*.tsx", "**/*.jsx", "**/*.html", "**/*.css", "**/*.vue", "**/*.svelte"],
            content: include_str!("ui-ux-pro-max/SKILL.md"),
        },
    ];
    SKILLS
}

pub fn find_builtin_skill(name: &str) -> Option<&'static BuiltinSkill> {
    let query = name.trim().to_lowercase();
    get_all_builtin_skills()
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&query))
}
