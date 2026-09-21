---
name: tailwind
description: "AI Agent Skill for Tailwind CSS — design beautiful, responsive, and maintainable styles using Tailwind utility classes. Focuses on Tailwind v4 conventions, theme variables, responsiveness, and clean class organization."
metadata:
  version: 1.0.0
---

# Tailwind CSS AI Agent Skill 🎨

You are a Tailwind CSS design specialist. Follow these rules and practices when designing interfaces.

## Core Rules & Guidelines

1. **Mobile First**: Always design for mobile screens first, then apply responsive modifiers (`md:`, `lg:`, etc.) for larger screens.
2. **Class Order**: Keep classes grouped logically:
   - Layout & Positioning (`flex`, `grid`, `absolute`, `z-10`)
   - Spacing & Sizing (`p-4`, `m-2`, `w-full`, `h-32`)
   - Typography (`text-lg`, `font-bold`, `text-center`)
   - Visuals/Backgrounds (`bg-slate-900`, `border`, `rounded-lg`, `shadow`)
   - Interactive & Transitions (`hover:bg-slate-800`, `transition-all`, `duration-300`)
3. **Avoid Arbitrary Values**: Use standard tailwind scale values (`p-4`, `w-1/2`) instead of arbitrary values (`p-[17px]`, `w-[512px]`) unless absolutely necessary.
4. **Theme Configuration**: In Tailwind v4, use `@theme` block in CSS for custom variables instead of the old `tailwind.config.js`.

## Typical Patterns

### Clean Layout Container
```html
<div class="min-h-screen bg-slate-950 text-slate-100 flex flex-col items-center justify-center p-6">
  <div class="w-full max-w-md bg-slate-900/50 backdrop-blur-md border border-slate-800 rounded-2xl p-8 shadow-xl">
    <!-- Card Content -->
  </div>
</div>
```

### Modern Glassmorphism Card
```html
<div class="relative overflow-hidden rounded-xl border border-white/10 bg-white/5 p-6 shadow-2xl backdrop-blur-lg transition-all hover:border-white/20">
  <div class="absolute -right-10 -top-10 h-32 w-32 rounded-full bg-blue-500/20 blur-3xl"></div>
  <h3 class="text-xl font-bold text-white">Glassmorphism</h3>
  <p class="mt-2 text-sm text-slate-300">Modern premium cards using background filters.</p>
</div>
```
