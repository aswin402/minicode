//! Architectural Subsystems & Feature Management for minicode.
//!
//! Exposes minicode's core capabilities (Interactive Inquiry, MiniKit, MiniTask,
//! MiniBlocks, MiniPowers, Obscura Browser, CodeGraph) through declarative
//! affordances, triggers, and runtime helpers rather than rigid turn-based scripts.

use serde::{Deserialize, Serialize};

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
            Self::Browser => "Dev Servers & Browser Verification",
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
                "kit_sync",
                "kit_skill_show",
            ],
            Self::MiniTask => &["create_plan", "update_progress", "archive_plan"],
            Self::MiniBlocks => &[
                "block_palettes",
                "block_search",
                "block_get",
                "block_insert",
                "block_scaffold",
                "block_import",
            ],
            Self::MiniPowers => &[
                "power_worktree_task",
                "power_plan",
                "power_review",
                "power_verify",
            ],
            Self::Browser => &[
                "minitask",
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
                "Provides structured methodologies (planning, Git worktree isolation, two-stage review, verification barriers) for complex tasks."
            }
            Self::Browser => {
                "Runs dev servers and daemons under supervision (`minitask`) and drives a browser to test web apps live, capture screenshots, and observe DOM visual health telemetry (fonts, stylesheets, layouts)."
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
                "When a decision is the user's to make (stack for a new project, visual direction, scope trade-off, credentials), is not settled by the request or the repo, and would be costly to reverse once work starts. Batch all such decisions into one call. Not for facts you can discover with tools, and not for cheap, easily changed details — state an assumption instead."
            }
            Self::MiniKit => {
                "When a new project needs a foundation and a matching starter stack exists (including `static-website` for plain HTML/CSS/JS). Scaffold with `kit_stack_add`, then customize. Not for adding packages to an existing project — use its package manager."
            }
            Self::MiniTask => {
                "When a task spans more than 1-2 trivial steps. Call `create_plan` to outline milestones and call `update_progress` as each milestone completes so the user has full execution visibility."
            }
            Self::MiniBlocks => {
                "When designing or building user interfaces. Query `block_palettes` for color tokens and `block_search`/`block_scaffold` for pre-built components (heroes, navbars, cards, modals) instead of inventing styles from memory."
            }
            Self::MiniPowers => {
                "When facing architectural doubt, risky refactors, or multi-step engineering initiatives. Use `power_plan` for structured plans, `power_review` for two-stage review, `power_worktree_task` for isolated risky work, and `power_verify` for 4-gate verification."
            }
            Self::Browser => {
                "When a project needs a running server (start it with `minitask(action=\"start\")` and keep it running across turns) and when building web UIs: navigate to the local dev server, capture screenshots, and verify visual health before concluding."
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
}
