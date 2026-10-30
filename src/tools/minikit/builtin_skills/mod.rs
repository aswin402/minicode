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
            globs: &["*.pdf", "posters/**/*", "canvas/**/*"],
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
        BuiltinSkill {
            name: "using-minipower",
            description: "MiniPower / obra/superpowers v7.0.1 Skill Routing & Anti-Rationalization Discipline — load process skills before domain skills via vault_load",
            globs: &["**/*"],
            content: r#"# Using MiniPower (obra/superpowers v7.0.1)
1. Invoke vault_load before acting: load process skills first (brainstorming, writing-plans, tdd-workflow, systematic-debugging), then domain skills.
2. Workers dispatched via power_worktree_task / spawn_agent execute their curated task brief directly without spawning sub-subagents."#,
        },
        BuiltinSkill {
            name: "brainstorming",
            description: "MiniPower / obra/superpowers v7.0.1 Brainstorming — ground-first codebase exploration, ceremony sizing, ask_user trade-offs, and cold-reader Builder Check via power_brainstorm",
            globs: &["minikit_docs/core/spec.md", "docs/**/*.md"],
            content: r#"# Brainstorming Ideas Into Designs (MiniPower / obra/superpowers v7.0.1)
1. Ground first via read_file/grep_search/symbol_search, size ceremony (quick_task/spike/small_change/project) via power_brainstorm, and clarify via ask_user.
2. Write approved spec to minikit_docs/core/spec.md, record [D1]..[DN] via record_decision, and run power_brainstorm(run_builder_check=true) for project-scale specs."#,
        },
        BuiltinSkill {
            name: "writing-plans",
            description: "MiniPower / obra/superpowers v7.0.1 Implementation Planning — bite-sized TDD tasks in .minicode/plan/task_plan.md & minikit_docs/core/todo.md with Consumes/Produces interfaces and 5-item Review Focus via power_plan",
            globs: &[".minicode/plan/task_plan.md", "minikit_docs/core/todo.md"],
            content: r#"# Writing Implementation Plans (MiniPower / obra/superpowers v7.0.1)
1. Use power_plan and create_plan to define bite-sized TDD tasks with explicit Interfaces (Consumes / Produces) and a 5-item Review Focus.
2. Run a 5-point self-review (spec coverage, zero placeholders, interface consistency, ordering, proportion check) before handoff."#,
        },
        BuiltinSkill {
            name: "executing-plans",
            description: "MiniPower / obra/superpowers v7.0.1 Inline Plan Execution — execute .minicode/plan/task_plan.md with update_progress, 'Rulings, not stalls' via record_decision, and power_verify",
            globs: &[".minicode/plan/task_plan.md", ".minicode/decisions.md"],
            content: r#"# Executing Plans Inline (MiniPower / obra/superpowers v7.0.1)
1. Execute tasks sequentially with update_progress and tdd-workflow, verifying each step with power_verify.
2. Make 'Rulings, not stalls' on minor ambiguities and log them via record_decision ([D1]..[DN]) in .minicode/decisions.md."#,
        },
        BuiltinSkill {
            name: "subagent-driven-development",
            description: "MiniPower / obra/superpowers v7.0.1 Subagent-Driven Execution — fresh isolated worker per task via power_worktree_task, 4-Status Report Contract, No-Subagents worker contract, and two-stage power_review with 5-round circuit breaker",
            globs: &[".minicode/plan/task_plan.md", "minikit_docs/core/todo.md"],
            content: r#"# Subagent-Driven Development (MiniPower / obra/superpowers v7.0.1)
1. Verify cross-task Consumes/Produces interfaces, then dispatch self-contained task briefs via power_worktree_task (workers never spawn subagents and report DONE | DONE_WITH_CONCERNS | NEEDS_CONTEXT | BLOCKED).
2. Run Stage 1 (spec) and Stage 2 (quality) power_review with Scoped Re-Review and a 5-round fix-loop circuit breaker."#,
        },
        BuiltinSkill {
            name: "dispatching-parallel-agents",
            description: "MiniPower / obra/superpowers v7.0.1 Parallel Agent Dispatch — partition independent failure domains and dispatch concurrent isolated workers via fanout_agents",
            globs: &["**/*"],
            content: r#"# Dispatching Parallel Agents (MiniPower / obra/superpowers v7.0.1)
1. Group independent tasks/failures with disjoint file ownership boundaries and dispatch via fanout_agents with worktree isolation.
2. Integrate diffs and run power_verify across the unified workspace."#,
        },
        BuiltinSkill {
            name: "tdd-workflow",
            description: "MiniPower / obra/superpowers v7.0.1 Falsifiable TDD — RED-GREEN-REFACTOR, Falsifiability Gate ('name the break it catches'), mutation checks, and full suite power_verify",
            globs: &["tests/**/*", "*_test.*", "*.spec.*", "*.test.*"],
            content: r#"# Falsifiable TDD & Writing Good Tests (MiniPower / obra/superpowers v7.0.1)
1. State 'This test catches <break> in <target>' before writing assertions; ban mirror assertions, change-detector tests, string-presence checks on scripts, and mock-existence tests.
2. Watch the test fail (RED), write minimal code (GREEN), run a mutation check, and run the full test suite via power_verify."#,
        },
        BuiltinSkill {
            name: "systematic-debugging",
            description: "MiniPower / obra/superpowers v7.0.1 Systematic Debugging — 4-phase root cause investigation, backward call-chain tracing, 4-layer defense-in-depth, and condition-based waiting",
            globs: &["**/*"],
            content: r#"# Systematic Debugging (MiniPower / obra/superpowers v7.0.1)
1. Trace backward up the call chain (grep_search / symbol_search) to the original source of bad state before proposing any fix.
2. Write a failing test, apply 4-layer defense-in-depth, replace sleep() with condition-based waiting, verify via power_verify, and log gotchas via vault_gotcha_add."#,
        },
        BuiltinSkill {
            name: "verification-before-completion",
            description: "MiniPower / obra/superpowers v7.0.1 Verification Gate — IDENTIFY -> RUN -> READ -> VERIFY -> CLAIM enforced via power_verify and .minicode/plan/last_verify.json",
            globs: &[".minicode/plan/last_verify.json", "**/*"],
            content: r#"# Verification Before Completion (MiniPower / obra/superpowers v7.0.1)
1. Never claim work is complete or tests pass without running power_verify after your final edit and reading the full output.
2. Confirm .minicode/plan/last_verify.json records passed: true with 0 failures before committing or finishing a branch."#,
        },
        BuiltinSkill {
            name: "requesting-code-review",
            description: "MiniPower / obra/superpowers v7.0.1 Two-Stage Code Review — Stage 1 Spec Compliance and Stage 2 Code Quality via power_review with git merge-base diffing and 'Declined to judge' reporting",
            globs: &["**/*"],
            content: r#"# Requesting Code Review (MiniPower / obra/superpowers v7.0.1)
1. Run power_review(stage='spec') then power_review(stage='quality') against the git merge-base diff range.
2. Treat the spec as a vision document for sensible concrete adaptations, do not re-run passing implementer tests, and surface any 'Declined to judge' items."#,
        },
        BuiltinSkill {
            name: "receiving-code-review",
            description: "MiniPower / obra/superpowers v7.0.1 Receiving Code Review — anti-sycophancy verification against codebase, YAGNI filtering, and technical pushback",
            globs: &["**/*"],
            content: r#"# Receiving Code Review (MiniPower / obra/superpowers v7.0.1)
1. Verify every review suggestion against the live codebase and .minicode/decisions.md before implementing; reject unused YAGNI abstractions.
2. Avoid performative agreement; implement valid fixes one at a time and verify each with power_verify."#,
        },
        BuiltinSkill {
            name: "using-git-worktrees",
            description: "MiniPower / obra/superpowers v7.0.1 Git Worktree Isolation — manage isolated feature/task worktrees via worktree_create, worktree_list, worktree_merge, worktree_remove, and power_worktree_task",
            globs: &[".git/**/*", ".worktrees/**/*"],
            content: r#"# Using Git Worktrees (MiniPower / obra/superpowers v7.0.1)
1. Create isolated worktrees via worktree_create or power_worktree_task and run power_verify immediately to confirm a clean baseline.
2. Merge verified work via worktree_merge and remove worktrees only after confirming zero untracked/uncommitted files."#,
        },
        BuiltinSkill {
            name: "finishing-a-development-branch",
            description: "MiniPower / obra/superpowers v7.0.1 Finishing a Branch — run power_verify, surface [D1]..[DN] rulings and deferred review notes, present 4 completion options, and safely clean up worktrees",
            globs: &[".minicode/plan/last_verify.json", ".minicode/decisions.md"],
            content: r#"# Finishing a Development Branch (MiniPower / obra/superpowers v7.0.1)
1. Confirm fresh power_verify pass, surface all [D1]..[DN] rulings from .minicode/decisions.md and deferred review minors, and present the 4 completion options (Merge locally, Push PR, Keep as-is, Discard).
2. Archive plan via archive_plan and check git status --porcelain before removing worktrees."#,
        },
        BuiltinSkill {
            name: "writing-skills",
            description: "MiniPower / obra/superpowers v7.0.1 Skill TDD — author and test skills via pressure scenarios, 'Use when...' CSO descriptions, and <20KB compaction limits using vault_create",
            globs: &[".minicode/skills/**/*", "**/SKILL.md"],
            content: r#"# Writing Skills via TDD (MiniPower / obra/superpowers v7.0.1)
1. Run a baseline pressure scenario (RED), author the skill via vault_create with a 'Use when...' CSO description and <20,000 byte size limit (GREEN), and close rationalization loopholes (REFACTOR)."#,
        },
        BuiltinSkill {
            name: "diagnosing-minipower",
            description: "MiniPower / obra/superpowers v7.0.1 Session Forensics — 7-dimension workflow & transcript diagnostics with mandatory path:line citations via power_diagnose",
            globs: &[".minicode/plan/**/*", ".minicode/decisions.md", "minikit_docs/core/**/*"],
            content: r#"# Diagnosing MiniPower Sessions (MiniPower / obra/superpowers v7.0.1)
1. Run power_diagnose(complaint) to audit 7 dimensions (skill-timeline, plan-adherence, request-conflicts, quality-evidence, repeated-work, stumbles, cost-and-time).
2. Every diagnostic finding must cite an exact path:line or tool-call step index."#,
        },
        BuiltinSkill {
            name: "ask-user-inquiry",
            description: "Use whenever you need clarity, domain data, or information from the user, or when you do not know or lack confidence about what to build — never guess or assume; ask as many questions as needed with 4 to 6 options via ask_user",
            globs: &[".minicode/decisions.md", "minikit_docs/core/spec.md"],
            content: r#"# Socratic User Inquiry & Zero-Guessing Protocol (ask-user-inquiry)
1. Check discoverable workspace facts first (read_file, grep_search, locate_symbol, .minicode/decisions.md); never guess or silently assume when uncertain or missing user information.
2. Call ask_user with as many questions as needed and 4 to 6 distinct options per choice question (recommended option first with recommended=true), and persist settled choices via record_decision ([D1]..[DN])."#,
        },
        BuiltinSkill {
            name: "todo-planning",
            description: "Use when creating, updating, reconciling, or archiving two-tier project roadmaps (minikit_docs/core/todo.md) and active tactical execution plans (.minicode/plan/task_plan.md)",
            globs: &[".minicode/plan/task_plan.md", "minikit_docs/core/todo.md"],
            content: r#"# Two-Tier Todo & Tactical Plan Orchestration (todo-planning)
1. Maintain Tier 1 strategic milestones in minikit_docs/core/todo.md and Tier 2 tactical steps (naming explicit target files per step) in .minicode/plan/task_plan.md via create_plan / power_plan.
2. Advance live via update_progress immediately after each step finishes, and call archive_plan once power_verify passes."#,
        },
        BuiltinSkill {
            name: "miniblocks-ui",
            description: "Use when designing, scaffolding, styling, or saving reusable UI components, layouts, color palettes, and gradients with the MiniBlocks warehouse",
            globs: &["**/*.tsx", "**/*.jsx", "**/*.html", "**/*.css", "**/*.vue", "**/*.svelte"],
            content: r#"# MiniBlocks UI Component & Art Direction Warehouse (miniblocks-ui)
1. Select cohesive palettes via block_palettes / block_gradients and search reusable UI blocks via block_search / block_scaffold / block_insert before hand-writing UI.
2. Save newly crafted reusable components into the warehouse via block_save or refine existing blocks via block_update."#,
        },
        BuiltinSkill {
            name: "minipubs-delegation",
            description: "Use when delegating specialized research, planning, coding, UI design, testing, debugging, or security audits to the 14 MiniPubs specialist subagents or parallel worker waves",
            globs: &["**/*"],
            content: r#"# MiniPubs Specialist Subagent & Parallel Orchestration (minipubs-delegation)
1. Delegate focused tasks to the 14 specialist roles via minitask(action='spawn_minipub', role=..., task=..., todos=[...], until=...) and steer live via minitask(action='update_minipub').
2. Relay artifacts zero-copy via $RESULT[pub_id], slice turns via $TURN[pub_id:N], and dispatch parallel waves via fanout_agents."#,
        },
        BuiltinSkill {
            name: "minitask-runtime",
            description: "Use when starting or monitoring background dev servers, scheduling recurring cron jobs or one-shot timers, inspecting process telemetry, or running headless/GUI PageAgent browser interaction and QA verification",
            globs: &["**/*"],
            content: r#"# MiniTask Background Runtime, Scheduler & PageAgent Browser Verification (minitask-runtime)
1. Start dev servers asynchronously via minitask(action='start'), monitor via minitask(action='status'|'logs'|'resources'), and schedule timers/cron via minitask(action='schedule').
2. Verify live web apps via browser_navigate, browser_snapshot, browser_click, browser_fill, browser_select_option, browser_hover, browser_scroll, browser_batch, browser_screenshot, and browser_qa_audit, then close via browser_close."#,
        },
        BuiltinSkill {
            name: "minikit-scaffolding",
            description: "Use when bootstrapping a new project, inspecting or applying MiniKit starter stacks, checking architectural drift, synchronizing .minicode/minicode.md, or managing packages",
            globs: &["minikit.json", "onpkg.json", ".minicode/minicode.md", "minikit_docs/**/*"],
            content: r#"# MiniKit Scaffolding, Workspace Sync & Package Management (minikit-scaffolding)
1. Discover starter stacks via kit_stack_list / kit_stack_show and scaffold via kit_stack_add rather than creating boilerplate files from scratch.
2. Keep .minicode/minicode.md and minikit_docs/core/ synchronized via kit_sync and manage packages via kit_pkg_add / kit_pkg_remove."#,
        },
        BuiltinSkill {
            name: "ast-codegraph-navigation",
            description: "Use when exploring unfamiliar codebases, locating symbols, analyzing blast radius and call graphs, or performing structural AST search and multi-file refactoring",
            globs: &["**/*"],
            content: r#"# Structural AST, CodeGraph & Trigram Search Navigation (ast-codegraph-navigation)
1. Navigate symbols and call graphs via locate_symbol, hybrid_search, code_explore, code_trace, code_impact, and diff_impact before reading full files.
2. Use ast_grep_search and ast_grep_rewrite (dry_run=true preview first) for syntax-aware structural queries/rewrites, and grep_search for literal/regex text across docs, logs, and configs."#,
        },
    ];
    SKILLS
}

pub fn find_builtin_skill(name: &str) -> Option<&'static BuiltinSkill> {
    let query = name.trim().to_lowercase();
    let canonical = match query.as_str() {
        "using-superpowers" => "using-minipower",
        "test-driven-development" => "tdd-workflow",
        "diagnosing-superpowers" => "diagnosing-minipower",
        other => other,
    };
    get_all_builtin_skills()
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(canonical))
}
