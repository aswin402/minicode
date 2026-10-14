---
name: frontend-design
description: "Design and build distinctive, production-grade frontend interfaces, landing pages, dashboards, and interactive web applications with editorial typography, curated color palettes, tactile motion, and clean modular architecture."
kind: "workflow"
category: "frontend"
globs: ["**/*.tsx", "**/*.jsx", "**/*.html", "**/*.css", "**/*.vue", "**/*.svelte"]
triggers: ["landing page", "frontend", "ui", "ux", "design", "css", "layout", "responsive", "website", "dashboard", "page"]
metadata:
  version: 2.0.0
---

# Frontend Design & Aesthetic Engineering

Build distinctive, memorable, production-grade user interfaces that avoid generic AI templates. Every interface must have a clear aesthetic point of view, cohesive color tokens, editorial typography, and interactive polish.

---

## 1. Aesthetic Direction & Design Tokens

Before writing component markup, establish a cohesive visual language:
- **Palette Selection (`block_palettes`)**: Query `block_palettes` for curated industry-specific color palettes (surface backgrounds, elevated cards, muted borders, primary/accent tokens, semantic status colors) and inject CSS custom properties (`--bg-primary`, `--surface-elevated`, `--border-subtle`, `--accent-primary`) in your global stylesheet.
- **Component Accelerators (`block_search` & `block_scaffold`)**: Search `block_search` for production-tested UI patterns (hero sections, bento grids, pricing matrices, testimonials, interactive navbars) and adapt them to the project's domain rather than writing boilerplate from scratch.
- **Intentional Theme Polarity**:
  - **High-Craft Dark UI**: Deep carbon/obsidian canvas (`#090A0F` to `#0D1117`), elevated slate surfaces (`#131722`) with `1px solid rgba(255,255,255,0.08)` hairline borders, crisp alabaster primary text (`#F8FAFC`), and cool slate muted text (`#94A3B8`).
  - **Editorial Light UI**: Warm ivory/alabaster canvas (`#FAF8F5` or `#F8FAFC`), pure surface cards (`#FFFFFF`) with subtle border (`#E2E8F0`), deep ink text (`#0F172A`), and restrained accent highlights.

---

## 2. Typography & Scannable Hierarchy

- **Font Pairing**: Pair an expressive display/geometric heading font (`Space Grotesk`, `Plus Jakarta Sans`, `Syne`, `Outfit`, or `Instrument Serif`) with a highly legible UI body font (`Inter`, `DM Sans`) and a crisp monospace font (`JetBrains Mono`, `Fira Code`) for metrics, code, and badges.
- **Scale & Tracking**:
  - Hero Headline: `clamp(2.5rem, 5vw, 4.25rem)`, `font-weight: 700–800`, tight tracking (`-0.03em`), line-height `1.08`.
  - Section Header: `clamp(1.75rem, 3vw, 2.5rem)`, `font-weight: 700`, tracking `-0.02em`.
  - Eyebrow / Kicker Badge: `0.75rem` (`12px`), uppercase, letter-spacing `0.08em`, monospace or semi-bold, pill container with subtle border.
  - Body Copy: `1rem–1.125rem` (`16px–18px`), line-height `1.65`, constrained to `60ch–70ch` max-width for effortless reading.
- **Numeric Discipline**: Always apply `tabular-nums` (`font-variant-numeric: tabular-nums`) to counters, pricing tables, live metrics, and timestamps.

---

## 3. Spatial Architecture & Viewport Budget

- **Above-the-Fold Discipline**: Maximum 2 distinct component groups visible without scrolling (e.g., Sticky Glass Navbar + Hero Split/Centered Section). Never stack multiple heavy banners above the fold.
- **Fixed Navbar Clearance**: When using a `fixed` or `sticky` top navigation bar (`h-16` / `64px` or `h-20` / `80px`), always give the hero section sufficient top padding (`pt-28` to `pt-36` / `112px–144px`) so the headline or eyebrow badge never collides with the header.
- **Bento Grids & Card Density**:
  - Maximum 3–4 data points per card (Icon/Badge, Title, 2-line description, optional metric or micro-preview).
  - Maximum 3 cards in a single desktop row (`grid-cols-1 md:grid-cols-2 lg:grid-cols-3`, `gap-6` / `24px`).
  - Maintain at least 25% open canvas/whitespace across the viewport (`py-20` to `py-28` vertical section rhythm).
- **Performant Ambient Glows**: Use lightweight CSS `radial-gradient(...)` backgrounds for atmospheric depth instead of oversized `filter: blur(128px)` DOM nodes that cause GPU repaint lag and text contrast washout.

---

## 4. Interactive Craft & Functional Completeness

- **Real Interactive State**: Every filter tab, billing toggle (Monthly/Annual), interactive demo control, command palette, or modal trigger MUST be wired to real state (`useState` / reactive state) with visible feedback.
- **Micro-Interactions**:
  - Cards: Subtle border luminance shift and `translateY(-2px)` lift on `:hover` (`transition-all duration-200`).
  - Buttons: Clear `:hover`, `:focus-visible` (`ring-2 ring-offset-2`), and `:active` (`scale-[0.98]`) states.
- **Iconography Hygiene**:
  - Use `lucide-react` (or inline SVG) for interface icons (`Terminal`, `Cpu`, `Sparkles`, `Shield`, `Zap`, `Check`, `ArrowRight`).
  - **NEVER import deprecated brand icons** (`Github`, `Twitter`, `Linkedin`, `Discord`, `Slack`) from `lucide-react` — they do not exist in modern `lucide-react` releases and will crash TypeScript compilation. Always render brand/social logos as clean inline `<svg viewBox="0 0 24 24">` components.
- **Modular Component Architecture**: Split complex pages into focused section components under `src/components/` (`Navbar.tsx`, `HeroSection.tsx`, `BentoFeatures.tsx`, `InteractiveDemo.tsx`, `PricingSection.tsx`, `Footer.tsx`), keeping each file under 200 lines.
