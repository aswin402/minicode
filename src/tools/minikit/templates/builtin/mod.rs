#![allow(clippy::all, unused)]

mod express;
mod fastapi;
mod flutter;
mod hono;
mod mern;
mod next_template;
mod pern;
mod react_vite;
mod rust_cli;
mod static_website;

use crate::tools::minikit::stacks::Stack;

pub fn builtin_stacks() -> Vec<Stack> {
    vec![
        // ── Bun / React ───────────────────────────────────────────
        react_vite::react_vite(),
        react_vite::react_vite_full(),
        react_vite::react_vite_gsap(),
        // ── Bun / API ─────────────────────────────────────────────
        hono::hono_api(),
        hono::hono_full(),
        express::express_api(),
        // ── Bun / Fullstack ───────────────────────────────────────
        next_template::next_template(),
        mern::mern(),
        pern::pern(),
        // ── Python ────────────────────────────────────────────────
        fastapi::fastapi(),
        // ── Rust ──────────────────────────────────────────────────
        rust_cli::rust_cli(),
        // ── Static Web ────────────────────────────────────────────
        static_website::static_website(),
        // ── Flutter ───────────────────────────────────────────────
        flutter::flutter_riverpod(None), // name: "flutter-riverpod"
    ]
}
