#![allow(clippy::all, unused)]

mod fastapi;
mod flutter;
mod hono;
mod mern;
mod next_template;
mod pern;
mod react_vite;

use crate::tools::onpkg::stacks::Stack;

pub fn builtin_stacks() -> Vec<Stack> {
    vec![
        // ── Bun / React ───────────────────────────────────────────
        react_vite::react_vite(),
        react_vite::react_vite_full(),
        react_vite::react_vite_gsap(),
        // ── Bun / API ─────────────────────────────────────────────
        hono::hono_api(),
        hono::hono_full(),
        // ── Bun / Fullstack ───────────────────────────────────────
        next_template::next_template(),
        mern::mern(),
        pern::pern(),
        // ── Python ────────────────────────────────────────────────
        fastapi::fastapi(),
        // ── Flutter ───────────────────────────────────────────────
        flutter::flutter_riverpod(None), // name: "flutter-riverpod"
    ]
}
