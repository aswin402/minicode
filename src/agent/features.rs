//! Architectural Subsystems & Feature Management for minicode.
//!
//! Exposes minicode's core capabilities (Interactive Inquiry, MiniKit, MiniTask,
//! MiniBlocks, MiniPowers, Obscura Browser, CodeGraph) through declarative
//! affordances, triggers, and runtime helpers rather than rigid turn-based scripts.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Core architectural feature pillars in minicode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentFeature {
    /// Human-in-the-loop interactive consultation (`ask_user`).
    Inquiry,
    /// Full-stack scaffolding, starter templates, docs, packages (`kit_*`).
    MiniKit,
    /// Task decomposition, progressive milestone tracking (`create_plan`, `update_progress`).
    MiniTask,
    /// Verified UI component warehouse & design token palettes (`block_*`).
    MiniBlocks,
    /// Autonomous engineering workflows, brainstorming, councils (`power_*`).
    MiniPowers,
    /// Web automation, DOM health observation, screenshots (`browser_*`).
    Browser,
    /// Tree-sitter AST symbol indexing & PageRank centrality (`locate_symbol`, `code_explore`).
    CodeGraph,
}

impl AgentFeature {
    /// All available feature subsystems in minicode.
    pub const ALL: &'static [AgentFeature] = &[
        AgentFeature::Inquiry,
        AgentFeature::MiniKit,
        AgentFeature::MiniTask,
        AgentFeature::MiniBlocks,
        AgentFeature::MiniPowers,
        AgentFeature::Browser,
        AgentFeature::CodeGraph,
    ];

    /// Canonical display name of the subsystem feature.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Inquiry => "Interactive Inquiry & Human-in-the-Loop",
            Self::MiniKit => "MiniKit Architecture & Scaffolding",
            Self::MiniTask => "MiniTask Planning & Progress Tracking",
            Self::MiniBlocks => "MiniBlocks UI & Design Token Warehouse",
            Self::MiniPowers => "MiniPowers Autonomous Engineering Workflows",
            Self::Browser => "Obscura Browser Automation & Visual Observability",
            Self::CodeGraph => "CodeGraph AST Intelligence & Centrality",
        }
    }

    /// Primary tools associated with this feature subsystem.
    pub fn tools(&self) -> &'static [&'static str] {
        match self {
            Self::Inquiry => &["ask_user"],
            Self::MiniKit => &[
                "kit_stack_list",
                "kit_stack_add",
                "kit_stack_show",
                "kit_stack_diff",
                "kit_info",
                "kit_add",
                "kit_remove",
                "kit_sync",
                "kit_skill_show",
            ],
            Self::MiniTask => &["create_plan", "update_progress", "minitask"],
            Self::MiniBlocks => &[
                "block_palettes",
                "block_search",
                "block_get",
                "block_insert",
                "block_scaffold",
                "block_import",
            ],
            Self::MiniPowers => &[
                "power_brainstorm",
                "power_worktree_task",
                "power_plan",
                "power_review",
                "power_verify",
            ],
            Self::Browser => &[
                "browser_navigate",
                "browser_screenshot",
                "browser_click",
                "browser_type",
                "browser_close",
            ],
            Self::CodeGraph => &[
                "locate_symbol",
                "code_explore",
                "blast_radius",
                "diff_impact",
            ],
        }
    }

    /// High-level purpose and intent.
    pub fn purpose(&self) -> &'static str {
        match self {
            Self::Inquiry => {
                "Direct, ambient communication channel to the user to clarify intent, confirm choices, or resolve trade-offs at any turn."
            }
            Self::MiniKit => {
                "Scaffolds complete, verified application stacks, generates structured specifications in minikit_docs/core/, and manages dependencies in 1 tool call."
            }
            Self::MiniTask => {
                "Decomposes multi-step tasks into atomic 2-5 minute verifiable milestones, providing real-time progress visualization in the live TUI dock."
            }
            Self::MiniBlocks => {
                "Provides 105+ verified color palette tokens and 1,080+ pre-built, tested UI components to prevent writing unstyled or monolithic CSS/JSX from scratch."
            }
            Self::MiniPowers => {
                "Provides structured methodologies (Socratic brainstorming, Git worktree isolation, Red/Green TDD, verification barriers) for complex tasks."
            }
            Self::Browser => {
                "Automates headless or visual browsers to test web apps live, capture screenshots, and observe DOM visual health telemetry (fonts, stylesheets, layouts)."
            }
            Self::CodeGraph => {
                "Indexes AST symbols, call hierarchies, and PageRank architectural centrality across the codebase for grounded navigation."
            }
        }
    }

    /// Declarative usage triggers — when should minicode reach for this feature.
    pub fn triggers(&self) -> &'static str {
        match self {
            Self::Inquiry => {
                "When requirements, framework choice, or aesthetic direction are open-ended; when choosing between multiple valid architectures; when an unexpected trade-off or breaking change arises; or when needing user-specific data/credentials. Use at ANY turn rather than guessing or assuming."
            }
            Self::MiniKit => {
                "When starting a new project, website, service, or feature. Call `kit_stack_add` to establish the codebase foundation instead of hand-authoring package manifests and configuration files from scratch."
            }
            Self::MiniTask => {
                "When a task spans more than 1-2 trivial steps. Call `create_plan` to outline milestones and call `update_progress` as each milestone completes so the user has full execution visibility."
            }
            Self::MiniBlocks => {
                "When designing or building user interfaces. Query `block_palettes` for color tokens and `block_search`/`block_scaffold` for pre-built components (heroes, navbars, cards, modals) instead of inventing styles from memory."
            }
            Self::MiniPowers => {
                "When facing architectural doubt, risky refactors, or multi-step engineering initiatives. Use `power_brainstorm` to explore trade-offs and `power_verify` for 4-gate verification."
            }
            Self::Browser => {
                "When building or modifying web pages and web applications. Navigate to the local dev server, capture screenshots, and verify visual health before concluding work."
            }
            Self::CodeGraph => {
                "When exploring an existing codebase, planning a refactor, or assessing the impact of changes across modules."
            }
        }
    }
}

/// Formats the complete declarative feature catalog for the agent system prompt.
pub fn format_features_catalog() -> String {
    let mut out = String::from("# Built-in Subsystems & Feature Management (Agent Affordances):\n");
    out.push_str("minicode provides first-class architectural subsystems designed to empower your problem-solving.\n");
    out.push_str("You have full freedom to tap into these capabilities whenever appropriate at any point in your workflow:\n\n");

    for feature in AgentFeature::ALL {
        out.push_str(&format!(
            "### {}. {}\n",
            feature_index(feature),
            feature.name()
        ));
        out.push_str(&format!("• **Purpose**: {}\n", feature.purpose()));
        out.push_str(&format!("• **When to Use**: {}\n", feature.triggers()));
        out.push_str(&format!(
            "• **Core Tools**: `{}`\n\n",
            feature.tools().join("`, `")
        ));
    }

    out
}

fn feature_index(feature: &AgentFeature) -> usize {
    match feature {
        AgentFeature::Inquiry => 1,
        AgentFeature::MiniKit => 2,
        AgentFeature::MiniTask => 3,
        AgentFeature::MiniBlocks => 4,
        AgentFeature::MiniPowers => 5,
        AgentFeature::Browser => 6,
        AgentFeature::CodeGraph => 7,
    }
}

/// Generates dynamic runtime feature helpers tailored to the current context.
///
/// Unlike rigid imperative scripts ("YOU MUST CALL X ON TURN 1"), these helpers
/// inform the model of high-leverage opportunities while preserving full agent freedom.
pub fn generate_context_helpers(
    workspace_root: &Path,
    prompt: &str,
    is_fresh_workspace: bool,
) -> Vec<String> {
    let lower = prompt.to_lowercase();
    let mut helpers = Vec::new();

    // 1. Inquiry Helper: Greenfield or open-ended stack
    let has_explicit_stack = crate::agent::orchestrator::has_any_word(
        &lower,
        &[
            "react", "vite", "nextjs", "vue", "svelte", "fastapi", "flask", "django", "express",
            "hono", "actix", "axum", "astro", "vanilla", "html", "tailwind",
        ],
    ) || lower.contains("next.js")
        || lower.contains("nextjs");

    if is_fresh_workspace && !has_explicit_stack {
        helpers.push(
            "• 💡 Feature Helper (Interactive Inquiry & MiniKit): Fresh workspace with no explicit tech stack or framework specified. \
            Invoke `ask_user` now to present 2-3 concrete stack options (e.g. React+Vite+Tailwind, Next.js, or Modern Static HTML) \
            and visual theme choices. Do not guess or unilaterally assume a framework without user confirmation.".to_string()
        );
    } else if is_fresh_workspace {
        helpers.push(
            "• 🚀 Feature Helper (MiniKit Scaffolding): Fresh workspace detected. Prefer scaffolding a verified stack template \
            with `kit_stack_add` (e.g. `kit_stack_add(stack_name=\"react-vite\")`) into the workspace root rather than manually authoring boilerplates.".to_string()
        );
    }

    // 2. MiniBlocks Helper: UI design or component creation
    let is_ui_task = crate::agent::orchestrator::has_any_word(
        &lower,
        &[
            "landing",
            "website",
            "dashboard",
            "component",
            "ui",
            "css",
            "frontend",
            "button",
            "modal",
            "navbar",
            "hero",
            "card",
            "theme",
            "palette",
        ],
    ) || lower.contains("landing page");
    if is_ui_task {
        let store_lock = crate::blocks::get_global_block_store();
        let store = match store_lock.read() {
            Ok(s) => Some(s),
            Err(e) => Some(e.into_inner()),
        };

        let mut palette_hint = String::new();
        if let Some(store) = store {
            let theme_words: Vec<&str> = [
                "cyberpunk",
                "futuristic",
                "dark",
                "neon",
                "minimal",
                "modern",
                "retro",
                "light",
                "monochrome",
            ]
            .iter()
            .filter(|&&w| lower.contains(w))
            .copied()
            .collect();
            let query = if !theme_words.is_empty() {
                theme_words.join(" ")
            } else {
                "dark".to_string()
            };
            let mut matches = store.search_palettes(&query);
            if matches.is_empty() {
                matches = store.search_palettes("dark");
            }
            if let Some(p) = matches.first() {
                palette_hint = format!(
                    " Verified theme palette '{}' [Tokens: --bg: {}, --surface: {}, --accent: {}, --text: {}]. \
                    Use these verified tokens in your styles or call `block_scaffold(palette=\"{}\")` for pre-themed components.",
                    p.name, p.colors[0], p.colors[1], p.colors[2], p.colors[3], p.name
                );
            }
        }

        helpers.push(format!(
            "• 🎨 Feature Helper (MiniBlocks): UI creation detected.{} \
            Query `block_search` / `block_scaffold` for pre-built components instead of inventing CSS/JSX from scratch.",
            palette_hint
        ));
    }

    // 3. MiniTask Helper: Multi-step task or complex feature
    let has_multi_steps = prompt.lines().count() > 3
        || prompt.contains("1.")
        || prompt.contains("*")
        || prompt.len() > 300;
    let core_todo = crate::tools::minikit::resolve_core_docs_dir(workspace_root).join("todo.md");
    let has_todo = core_todo.exists() || workspace_root.join("todo.md").exists();

    if has_multi_steps && !has_todo {
        helpers.push(
            "• 📋 Feature Helper (MiniTask Planning): Multi-step work detected. Call `create_plan` to structure tasks into \
            verifiable steps. Calling `update_progress` as you finish steps keeps the user informed in real-time in the live TUI.".to_string()
        );
    }

    // 4. Browser Automation Helper: Web/frontend verification
    if is_ui_task {
        helpers.push(
            "• 🌐 Feature Helper (Obscura Browser): When testing web pages, navigate via `browser_navigate` and inspect \
            the returned `[DOM & Visual Health Observation]` telemetry to confirm stylesheets and typography render cleanly.".to_string()
        );
    }

    helpers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_features_catalog_contains_all_features() {
        let catalog = format_features_catalog();
        for feature in AgentFeature::ALL {
            assert!(catalog.contains(feature.name()));
            assert!(catalog.contains(feature.purpose()));
        }
    }

    #[test]
    fn test_context_helpers_for_fresh_workspace_no_stack() {
        let helpers = generate_context_helpers(
            Path::new("."),
            "Create a modern landing page for minicode",
            true,
        );
        assert!(helpers.iter().any(|h| h.contains("Interactive Inquiry")));
        assert!(helpers.iter().any(|h| h.contains("MiniBlocks")));
    }

    #[test]
    fn test_context_helpers_for_fresh_workspace_with_stack() {
        let helpers = generate_context_helpers(
            Path::new("."),
            "Create a React Vite landing page with Tailwind",
            true,
        );
        assert!(helpers.iter().any(|h| h.contains("MiniKit Scaffolding")));
        assert!(helpers.iter().any(|h| h.contains("MiniBlocks")));
    }
}
