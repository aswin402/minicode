use crate::tools::category::ToolCategory;
use std::collections::HashSet;

/// Declarative rule mapping semantic tokens, phrases, and co-occurrence pairs to `ToolCategory` targets.
struct IntentRule {
    categories: &'static [ToolCategory],
    exact_words: &'static [&'static str],
    phrases: &'static [&'static str],
    co_occurrences: &'static [(&'static [&'static str], &'static [&'static str])],
    exact_prompts: &'static [&'static str],
}

const INTENT_RULES: &[IntentRule] = &[
    // 1. Git
    IntentRule {
        categories: &[ToolCategory::Git],
        exact_words: &["git", "commit", "branch", "diff", "stash", "merge", "pr"],
        phrases: &["pull request"],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 2. Web & Browser
    IntentRule {
        categories: &[ToolCategory::Web],
        exact_words: &["browser", "browse", "crawl", "screenshot", "website"],
        phrases: &[
            "http://",
            "https://",
            "search web",
            "web search",
            "online docs",
            "latest docs",
            "documentation for",
        ],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 3. MiniKit Architecture Stacks, Dependencies & Skills
    IntentRule {
        categories: &[ToolCategory::MiniKit],
        exact_words: &[
            "minikit",
            "onpkg",
            "scaffold",
            "template",
            "bootstrap",
            "drift",
            "stack",
            "skill",
            "skills",
            "package",
            "pkg",
            "dependency",
            "dependencies",
            "npm",
            "pnpm",
        ],
        phrases: &[
            "kit ",
            "kit_",
            "self-heal",
            "add dep",
            "install dep",
            "librar",
            "yarn add",
            "bun add",
            "cargo add",
            "pip install",
            "uv add",
            "flutter pub",
        ],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 4. MiniVault Agent Skills & Bundles
    IntentRule {
        categories: &[ToolCategory::Vault],
        exact_words: &[
            "vault",
            "minivault",
            "skill",
            "skills",
            "bundle",
            "bundles",
            "vault_search",
            "vault_show",
            "vault_load",
        ],
        phrases: &["skill bundle", "load skill", "agent skill"],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 5. CodeGraph & Architecture
    IntentRule {
        categories: &[
            ToolCategory::Codegraph,
            ToolCategory::Memory,
            ToolCategory::Search,
        ],
        exact_words: &[
            "architecture",
            "codegraph",
            "repo_map",
            "repomap",
            "code_explore",
            "diff_impact",
            "code_explain",
            "code_trace",
            "code_impact",
            "callers",
            "callees",
        ],
        phrases: &[
            "blast radius",
            "call hierarchy",
            "dependency graph",
            "impact analysis",
            "impact of",
            "code graph",
            "who calls",
            "explain symbol",
            "call trace",
            "trace flow",
            "trace execution",
            "where is",
            "how does",
        ],
        co_occurrences: &[(&["trace"], &["call", "function", "symbol", "flow"])],
        exact_prompts: &[],
    },
    // 6. Multi-Agent & MiniPub Swarm
    IntentRule {
        categories: &[ToolCategory::Agent],
        exact_words: &[
            "minipub",
            "subagent",
            "subagents",
            "swarm",
            "council",
            "consensus",
            "delegate",
            "fanout",
            "scratchpad",
            "send_worker_message",
            "worker_message",
            "peer",
            "coordination",
            "publish_contract",
            "query_interface",
        ],
        phrases: &["hypothes"],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 7. AST & Deep Search
    IntentRule {
        categories: &[ToolCategory::Search],
        exact_words: &[
            "ast",
            "ast_grep",
            "ast_grep_search",
            "ast_grep_rewrite",
            "codemod",
            "lsp",
            "hybrid_search",
            "semantic_search",
            "locate_fault",
            "fault",
        ],
        phrases: &[
            "syntax tree",
            "goto definition",
            "find references",
            "hybrid search",
            "semantic search",
        ],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 8. Context, Memory, Architecture & Analysis
    IntentRule {
        categories: &[ToolCategory::Memory],
        exact_words: &[
            "wiki",
            "remember",
            "memory",
            "plan",
            "progress",
            "repo_map",
            "repomap",
            "skeleton",
            "smell",
            "code_smells",
            "dead_code",
            "invariant",
            "prune",
            "skill",
            "skills",
        ],
        phrases: &[
            "forget fact",
            "code map",
            "dead code",
            "coverage gap",
            "token budget",
        ],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 9. MiniPower Autonomous Methodology & Verification
    IntentRule {
        categories: &[ToolCategory::MiniPower, ToolCategory::Memory],
        exact_words: &[
            "power",
            "minipower",
            "superpower",
            "verify",
            "verification",
            "barrier",
            "review",
            "brainstorm",
            "socratic",
            "tdd",
            "worktree",
            "compliance",
            "implement",
            "refactor",
            "feature",
            "milestone",
        ],
        phrases: &["red flag", "acceptance criteria", "end-to-end"],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 10. MiniBlocks UI Component & Design Warehouse
    IntentRule {
        categories: &[ToolCategory::Blocks, ToolCategory::MiniKit],
        exact_words: &[
            "block",
            "blocks",
            "miniblocks",
            "component",
            "components",
            "palette",
            "palettes",
            "gradient",
            "navbar",
            "hero",
            "landing",
            "dashboard",
            "sidebar",
            "pricing",
            "modal",
            "dialog",
            "tailwind",
            "responsive",
            "ui",
            "frontend",
            "css",
            "styling",
            "button",
            "card",
            "table",
            "footer",
            "header",
            "wireframe",
        ],
        phrases: &["ui design", "design token", "dark mode", "light mode"],
        co_occurrences: &[],
        exact_prompts: &[],
    },
    // 11. MiniTask Process Vault & Resource Telemetry
    IntentRule {
        categories: &[ToolCategory::Dev, ToolCategory::Exec],
        exact_words: &[
            "minitask",
            "daemon",
            "vite",
            "serve",
            "resource",
            "telemetry",
            "ram",
            "cpu",
        ],
        phrases: &[
            "dev server",
            "task manager",
            "background process",
            "background task",
            "scheduled task",
            "one-shot timer",
            "periodic check",
            "listen on port",
            "port ",
            "memory usage",
            "whats happening",
            "what's happening",
            "what is happening",
            "what is running",
            "what's running",
            "whats running",
            "status of task",
            "task status",
            "process status",
            "is the server running",
            "is server running",
            "active processes",
            "active tasks",
            "check tasks",
            "check processes",
            "check background",
            "stop that",
            "kill that",
            "manage task",
        ],
        co_occurrences: &[
            (
                &["server"],
                &[
                    "start", "run", "launch", "serve", "status", "logs", "kill", "stop", "restart",
                ],
            ),
            (&["how", "what"], &["tasks", "processes"]),
        ],
        exact_prompts: &["stop", "kill", "stop it", "kill it"],
    },
];

const MCP_DOMAIN_RULES: &[(&[&str], &[&str])] = &[
    (
        &["figma"],
        &["frame", "component", "canvas", "design system", "ui design"],
    ),
    (
        &["github", "gh"],
        &["pull request", " pr ", "issue", "repo"],
    ),
    (
        &["postgres", "mysql", "sqlite", "database", "db", "sql"],
        &["sql", "query", "database", "migration", "schema"],
    ),
    (&["docker"], &["container", "dockerfile", "compose"]),
];

/// High-speed, declarative intent classifier for dynamic tool gating.
pub struct IntentClassifier;

#[inline]
fn tokenize_words(lower: &str) -> HashSet<&str> {
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect()
}

impl IntentClassifier {
    /// Detects relevant tool categories from user prompt text in a single declarative pass.
    pub fn detect(prompt: &str) -> HashSet<ToolCategory> {
        let mut categories = HashSet::new();
        let lower = prompt.to_ascii_lowercase();
        let trimmed = lower.trim();
        let words = tokenize_words(&lower);

        for rule in INTENT_RULES {
            let matched = rule.exact_words.iter().any(|w| words.contains(w))
                || rule.phrases.iter().any(|p| lower.contains(p))
                || rule.exact_prompts.contains(&trimmed)
                || rule.co_occurrences.iter().any(|(lhs, rhs)| {
                    lhs.iter().any(|l| words.contains(l)) && rhs.iter().any(|r| words.contains(r))
                });
            if matched {
                categories.extend(rule.categories.iter().copied());
            }
        }

        categories
    }

    /// Detects relevant MCP server names from user prompt text and available servers.
    #[must_use]
    pub fn detect_mcp_servers<I, S>(prompt: &str, available_servers: I) -> HashSet<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut matched = HashSet::new();
        let lower = prompt.to_ascii_lowercase();
        let words = tokenize_words(&lower);

        for server in available_servers {
            let s_ref = server.as_ref();
            let s_lower = s_ref.to_ascii_lowercase();
            if words.contains(s_lower.as_str()) || lower.contains(&s_lower) {
                matched.insert(s_ref.to_string());
                continue;
            }

            for (server_names, domain_terms) in MCP_DOMAIN_RULES {
                if server_names.contains(&s_lower.as_str())
                    && domain_terms
                        .iter()
                        .any(|term| words.contains(term) || lower.contains(term))
                {
                    matched.insert(s_ref.to_string());
                    break;
                }
            }
        }

        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_detection_empty_or_greeting() {
        let detected = IntentClassifier::detect("hello world");
        assert!(detected.is_empty());

        let detected = IntentClassifier::detect("hii minicode");
        assert!(detected.is_empty());
    }

    #[test]
    fn test_intent_detection_git() {
        let detected = IntentClassifier::detect("please commit these changes with git");
        assert!(detected.contains(&ToolCategory::Git));
    }

    #[test]
    fn test_intent_detection_web() {
        let detected = IntentClassifier::detect("search web for latest ratatui examples");
        assert!(detected.contains(&ToolCategory::Web));
    }

    #[test]
    fn test_intent_detection_codegraph() {
        let detected =
            IntentClassifier::detect("analyze the blast radius of changing this function");
        assert!(detected.contains(&ToolCategory::Codegraph));
    }

    #[test]
    fn test_intent_detection_minikit() {
        let detected = IntentClassifier::detect("scaffold a new stack with minikit");
        assert!(detected.contains(&ToolCategory::MiniKit));
        let detected2 = IntentClassifier::detect("scaffold a new stack with onpkg");
        assert!(detected2.contains(&ToolCategory::MiniKit));
    }

    #[test]
    fn test_intent_detection_minipower() {
        let detected =
            IntentClassifier::detect("verify these changes against the verification barrier");
        assert!(detected.contains(&ToolCategory::MiniPower));
        let detected2 =
            IntentClassifier::detect("execute this task in an isolated worktree with minipower");
        assert!(detected2.contains(&ToolCategory::MiniPower));
        let detected3 =
            IntentClassifier::detect("brainstorm the architecture first using socratic method");
        assert!(detected3.contains(&ToolCategory::MiniPower));
    }

    #[test]
    fn test_intent_detection_multiple() {
        let detected = IntentClassifier::detect("check git diff and search web for docs");
        assert!(detected.contains(&ToolCategory::Git));
        assert!(detected.contains(&ToolCategory::Web));
    }

    #[test]
    fn test_intent_detection_blocks() {
        let detected = IntentClassifier::detect("search for a responsive navbar component");
        assert!(detected.contains(&ToolCategory::Blocks));
        let detected2 = IntentClassifier::detect("show me modern color palettes with hex tokens");
        assert!(detected2.contains(&ToolCategory::Blocks));
        let detected3 = IntentClassifier::detect("scaffold a hero section from miniblocks");
        assert!(detected3.contains(&ToolCategory::Blocks));
        // Natural UI prompts without commands
        let detected4 = IntentClassifier::detect(
            "create a modern landing page with a pricing table in dark mode",
        );
        assert!(detected4.contains(&ToolCategory::Blocks));
        assert!(detected4.contains(&ToolCategory::MiniKit));
        let detected5 = IntentClassifier::detect("style the sidebar and button with tailwind");
        assert!(detected5.contains(&ToolCategory::Blocks));
    }

    #[test]
    fn test_intent_detection_natural_prompts() {
        // Natural AST / CodeGraph exploration
        let det_ast =
            IntentClassifier::detect("where is process_payment defined and who calls it?");
        assert!(det_ast.contains(&ToolCategory::Codegraph));
        assert!(det_ast.contains(&ToolCategory::Search));

        // Natural Dev server & resource queries
        let det_dev = IntentClassifier::detect("start the dev server on port 3000");
        assert!(det_dev.contains(&ToolCategory::Dev));
        assert!(det_dev.contains(&ToolCategory::Exec));

        let det_res = IntentClassifier::detect("so how much resource its taking");
        assert!(det_res.contains(&ToolCategory::Dev));

        let det_stop = IntentClassifier::detect("ok stop that");
        assert!(det_stop.contains(&ToolCategory::Dev));

        let det_browser = IntentClassifier::detect("launch website and close browser");
        assert!(det_browser.contains(&ToolCategory::Web));

        // Natural Engineering / TDD
        let det_eng = IntentClassifier::detect("implement user authentication feature with tests");
        assert!(det_eng.contains(&ToolCategory::MiniPower));
        assert!(det_eng.contains(&ToolCategory::Memory));
    }

    #[test]
    fn test_detect_mcp_servers() {
        let servers = vec!["figma", "github", "postgres"];

        // 1. Direct name match
        let matched = IntentClassifier::detect_mcp_servers("inspect figma document", &servers);
        assert!(matched.contains("figma"));
        assert!(!matched.contains("github"));

        // 2. Domain keywords
        let matched =
            IntentClassifier::detect_mcp_servers("create a new frame and component", &servers);
        assert!(matched.contains("figma"));

        let matched =
            IntentClassifier::detect_mcp_servers("open a pull request for this branch", &servers);
        assert!(matched.contains("github"));

        let matched =
            IntentClassifier::detect_mcp_servers("run this sql migration query", &servers);
        assert!(matched.contains("postgres"));

        // 3. No match
        let matched =
            IntentClassifier::detect_mcp_servers("fix compiler error in main.rs", &servers);
        assert!(matched.is_empty());
    }
}
