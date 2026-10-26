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
            globs: &["**/app/**/page.tsx", "**/app/**/layout.tsx", "**/app/**/route.ts", "**/pages/**/*.tsx", "**/next.config.*"],
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
        BuiltinSkill {
            name: "mcp-builder",
            description: "Official Anthropic MCP Server Builder — TypeScript (@modelcontextprotocol/sdk) & Python (FastMCP) servers, schemas, and evaluations",
            globs: &["*mcp*.ts", "*mcp*.py", "**/mcp/**/*.ts", "**/mcp/**/*.py", "**/mcp/**/*.rs"],
            content: r#"# MCP Server Development Guide (anthropics/skills)
1. Prefer Streamable HTTP for remote servers and stdio for local servers (never log to stdout in stdio servers).
2. Design high-signal workflow tools with strict Zod/Pydantic schemas, pagination, and actionable errors."#,
        },
        BuiltinSkill {
            name: "webapp-testing",
            description: "Official Anthropic Web Application Testing — Playwright reconnaissance-then-action E2E testing, DOM inspection, and console error verification",
            globs: &["playwright.config.*", "e2e/**/*", "tests/**/*.spec.ts", "tests/**/*.test.ts"],
            content: r#"# Web Application Testing Protocol (anthropics/skills)
1. Wait for networkidle on dynamic SPAs before inspecting DOM or taking screenshots.
2. Inspect rendered DOM locators before clicking and assert zero console/page errors."#,
        },
        BuiltinSkill {
            name: "skill-creator",
            description: "Official Anthropic Skill Creator — 3-level progressive disclosure skill authoring (SKILL.md, scripts/, references/, assets/)",
            globs: &["**/SKILL.md", ".minicode/skills/**/*"],
            content: r#"# Agent Skill Authoring Guide (anthropics/skills)
1. Level 1: Pushy YAML frontmatter (name, description, triggers, globs, allowed_tools).
2. Level 2: Imperative SKILL.md (<500 lines). Level 3: Bundled scripts/, references/, assets/."#,
        },
        BuiltinSkill {
            name: "doc-coauthoring",
            description: "Official Anthropic Document Co-Authoring — 3-stage technical spec, RFC, ADR, and PRD authoring workflow",
            globs: &["docs/**/*.md", "*.prd.md", "RFC*.md", "ADR*.md", "SPEC*.md"],
            content: r#"# Structured Document Co-Authoring (anthropics/skills)
1. Stage 1: Context Gathering & gap-filling questions.
2. Stage 2: Section-by-section outline & refinement. Stage 3: Cold-reader verification."#,
        },
        BuiltinSkill {
            name: "web-artifacts-builder",
            description: "Official Anthropic Web Artifacts Builder — multi-component interactive React + Tailwind + shadcn/ui single-file HTML bundles",
            globs: &["*.artifact.html", "*.bundle.html", "artifacts/**/*.tsx", "artifacts/**/*.html"],
            content: r#"# Web Artifacts Builder (anthropics/skills)
1. Avoid generic AI slop; use intentional Swiss grid, editorial, or high-craft dark UI aesthetics.
2. Bundle interactive React/Tailwind components into self-contained HTML artifacts and verify via headless browser."#,
        },
        BuiltinSkill {
            name: "canvas-design",
            description: "Official Anthropic Canvas Design — museum-grade visual philosophy manifesto and PNG/PDF/SVG poster & spatial composition",
            globs: &["*.svg", "*.pdf", "posters/**/*", "canvas/**/*"],
            content: r#"# Canvas Design & Spatial Composition (anthropics/skills)
1. Define a Visual Philosophy Manifesto before composing on canvas.
2. Enforce mathematical grid alignment, generous negative space, and zero text/shape overlaps."#,
        },
        BuiltinSkill {
            name: "algorithmic-art",
            description: "Official Anthropic Algorithmic Art — seeded p5.js & HTML5 Canvas generative flow fields, noise systems, and interactive parameters",
            globs: &["*sketch*.js", "*sketch*.html", "*p5*.html", "*generative*.html"],
            content: r#"# Algorithmic & Generative Art (anthropics/skills)
1. Express emergent mathematical beauty via noise fields, particle systems, and recursive subdivision.
2. Always use explicit randomSeed/noiseSeed reproducibility and interactive parameter controls."#,
        },
        BuiltinSkill {
            name: "theme-factory",
            description: "Official Anthropic Theme Factory — 10 curated color palettes and typography pairings plus custom theme generation",
            globs: &["**/theme*.*", "**/*.css", "**/*.pptx", "**/*.html"],
            content: r#"# Theme Factory (anthropics/skills)
1. Apply cohesive curated palettes (Ocean Depths, Sunset Boulevard, Forest Canopy, Modern Minimalist, Midnight Galaxy, etc.).
2. Enforce WCAG AA contrast and 80/20 neutral-to-accent surface ratios."#,
        },
        BuiltinSkill {
            name: "slack-gif-creator",
            description: "Official Anthropic Slack GIF Creator — 128x128 emoji and 480x480 message GIF synthesis with easing curves and palette optimization",
            globs: &["*.gif", "*gif*.py"],
            content: r#"# Slack Animated GIF Creator (anthropics/skills)
1. Use easing curves (ease_out_cubic, ease_out_back, bounce_out) and phase-locked seamless loops.
2. Quantize with adaptive palettes (64-128 colors) for lightweight Slack delivery."#,
        },
        BuiltinSkill {
            name: "claude-api",
            description: "Official Anthropic Claude API & SDK Guide — Messages API, adaptive thinking, streaming, tool use, and prompt caching",
            globs: &["*anthropic*.ts", "*anthropic*.py", "*claude*.ts", "*claude*.py"],
            content: r#"# Claude API & Anthropic SDK Guide (anthropics/skills)
1. Default to claude-sonnet-4-6 / claude-opus-4-6 with adaptive thinking and streaming (.stream()) for large outputs.
2. Use strict JSON schemas for tool use and ephemeral cache_control for static system/tool prefixes."#,
        },
        BuiltinSkill {
            name: "docx",
            description: "Official Anthropic Word (.docx) Skill — document creation (docx-js / python-docx), pandoc extraction, and OOXML tracked-changes redlining",
            globs: &["*.docx", "*docx*.js", "*docx*.ts", "*docx*.py"],
            content: r#"# Word (.docx) Creation & Redlining (anthropics/skills)
1. Extract text with pandoc --track-changes=all; create new docs with docx-js or python-docx.
2. For redlining, wrap minimal changed spans in <w:del> and <w:ins> while preserving unchanged <w:r> runs."#,
        },
        BuiltinSkill {
            name: "pdf",
            description: "Official Anthropic PDF Skill — text/table extraction (pdfplumber), merge/split (pypdf/qpdf), form filling, and reportlab generation",
            globs: &["*.pdf", "*pdf*.py"],
            content: r#"# PDF Processing & Generation (anthropics/skills)
1. Extract layout-preserving text/tables with pdfplumber; merge/split with pypdf.
2. Generate multi-page reports with reportlab and visually inspect rendered pages via pdftoppm."#,
        },
        BuiltinSkill {
            name: "pptx",
            description: "Official Anthropic PowerPoint (.pptx) Skill — slide deck generation (python-pptx), markitdown extraction, and visual layout hierarchy",
            globs: &["*.pptx", "*pptx*.py", "*slides*.py"],
            content: r#"# PowerPoint (.pptx) Presentation Engineering (anthropics/skills)
1. Choose a content-informed 3-5 color palette on 16:9 widescreen slides.
2. Keep one core idea per slide with strong typographic contrast and zero overlapping bounding boxes."#,
        },
        BuiltinSkill {
            name: "xlsx",
            description: "Official Anthropic Excel (.xlsx) Skill — dynamic openpyxl formulas, zero formula errors, and financial modeling color standards",
            globs: &["*.xlsx", "*.xlsm", "*excel*.py", "*xlsx*.py"],
            content: r#"# Excel (.xlsx) & Financial Modeling (anthropics/skills)
1. Always write live Excel formulas (=SUM, =INDEX/MATCH) rather than hardcoded Python calculations.
2. Deliver zero formula errors (#REF!, #DIV/0!) and apply standard financial color coding (blue inputs, black formulas, green cross-sheet links)."#,
        },
        BuiltinSkill {
            name: "brand-guidelines",
            description: "Official Anthropic Brand Guidelines — color tokens (#141413, #faf9f5, #d97757, #6a9bcc, #788c5d) and Poppins/Lora typography",
            globs: &["**/brand*.*", "**/*.css", "**/*.pptx", "**/*.html"],
            content: r#"# Anthropic Brand Guidelines (anthropics/skills)
1. Colors: Dark #141413, Light #FAF9F5, Mid Gray #B0AEA5, Light Gray #E8E6DC, Orange #D97757, Blue #6A9BCC, Green #788C5D.
2. Typography: Poppins (headings 24pt+) and Lora (editorial body) with Arial/Georgia fallbacks."#,
        },
        BuiltinSkill {
            name: "internal-comms",
            description: "Official Anthropic Internal Communications — 3P team updates (Progress, Plans, Problems), incident reports, and leadership briefs",
            globs: &["comms/**/*.md", "updates/**/*.md", "postmortem*.md"],
            content: r#"# Internal Engineering Communications (anthropics/skills)
1. 3P Updates: Lead with status badge + TL;DR, followed by outcome-focused Progress, prioritized Plans, and actionable Problems/Asks.
2. Incident Reports: Executive summary, UTC timeline, blameless 5-Whys root cause, and P0/P1 preventative action items."#,
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
