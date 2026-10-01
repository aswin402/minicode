use crate::tools::category::ToolCategory;
use std::collections::HashSet;

/// High-speed, zero-allocation intent classifier for dynamic tool gating.
pub struct IntentClassifier;

#[inline]
fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|&p| text.contains(p))
}

#[inline]
fn has_any_word(text: &str, words: &[&str]) -> bool {
    words.iter().any(|&w| crate::utils::has_word(text, w))
}

impl IntentClassifier {
    /// Detects relevant tool categories from user prompt text.
    /// Runs in < 0.1ms using structured semantic intent signals.
    pub fn detect(prompt: &str) -> HashSet<ToolCategory> {
        let mut categories = HashSet::new();
        let lower = prompt.to_ascii_lowercase();

        // 1. Git Intent
        let has_git_term = lower.starts_with("git")
            || lower.contains("git ")
            || lower.contains("pull request")
            || has_any_word(
                &lower,
                &["git", "commit", "branch", "diff", "stash", "merge", "pr"],
            );

        if has_git_term {
            categories.insert(ToolCategory::Git);
        }

        // 2. Web & Browser Intent
        let has_web_url = contains_any(&lower, &["http://", "https://"]);
        let has_web_search = contains_any(
            &lower,
            &[
                "search web",
                "web search",
                "online docs",
                "latest docs",
                "documentation for",
            ],
        );
        let has_browser_action =
            contains_any(&lower, &["browser", "browse", "crawl", "screenshot"])
                || crate::utils::has_word(&lower, "website");

        if has_web_url || has_web_search || has_browser_action {
            categories.insert(ToolCategory::Web);
        }

        // 3. MiniKit Architecture Stacks, Dependencies & Skills Intent
        let has_minikit_term = contains_any(
            &lower,
            &[
                "minikit",
                "kit ",
                "kit_",
                "onpkg",
                "scaffold",
                "template",
                "bootstrap",
                "drift",
                "self-heal",
            ],
        );
        let has_package_management = contains_any(
            &lower,
            &[
                "package",
                "pkg",
                "dependency",
                "dependencies",
                "add dep",
                "install dep",
                "librar",
                "npm",
                "yarn add",
                "pnpm",
                "bun add",
                "cargo add",
                "pip install",
                "uv add",
                "flutter pub",
            ],
        );
        let has_kit_word = has_any_word(&lower, &["stack", "skill", "skills"]);

        if has_minikit_term || has_package_management || has_kit_word {
            categories.insert(ToolCategory::MiniKit);
        }

        // 4. CodeGraph & Architecture Intent
        let has_graph_term = contains_any(
            &lower,
            &[
                "architecture",
                "blast radius",
                "call hierarchy",
                "dependency graph",
                "impact analysis",
                "impact of",
                "code graph",
                "codegraph",
                "repo_map",
                "repomap",
                "code_explore",
                "diff_impact",
                "code_explain",
                "code_trace",
                "code_impact",
            ],
        );
        let has_ast_query = contains_any(
            &lower,
            &[
                "who calls",
                "callers",
                "callees",
                "explain symbol",
                "call trace",
                "trace flow",
                "trace execution",
                "where is",
                "how does",
            ],
        );
        let has_trace_flow = crate::utils::has_word(&lower, "trace")
            && (crate::utils::has_word(&lower, "call")
                || crate::utils::has_word(&lower, "function")
                || crate::utils::has_word(&lower, "symbol")
                || crate::utils::has_word(&lower, "flow"));

        if has_graph_term || has_ast_query || has_trace_flow {
            categories.insert(ToolCategory::Codegraph);
            categories.insert(ToolCategory::Memory);
            categories.insert(ToolCategory::Search);
        }

        // 5. Multi-Agent & Swarm Intent
        if contains_any(
            &lower,
            &[
                "subagent",
                "swarm",
                "council",
                "consensus",
                "hypothes",
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
        ) {
            categories.insert(ToolCategory::Agent);
        }

        // 6. AST & Deep Search Intent
        let has_search_term = contains_any(
            &lower,
            &[
                "syntax tree",
                "goto definition",
                "find references",
                "hybrid search",
                "hybrid_search",
                "semantic search",
                "semantic_search",
                "locate_fault",
                "fault",
            ],
        );
        let has_search_word = has_any_word(&lower, &["ast", "lsp"]);

        if has_search_term || has_search_word {
            categories.insert(ToolCategory::Search);
        }

        // 7. Context, Memory, Architecture & Analysis Intent
        let has_memory_term = contains_any(
            &lower,
            &[
                "wiki",
                "remember",
                "forget fact",
                "memory",
                "plan",
                "progress",
                "repo_map",
                "repomap",
                "code map",
                "skeleton",
                "smell",
                "code_smells",
                "dead code",
                "dead_code",
                "invariant",
                "coverage gap",
                "prune",
                "token budget",
            ],
        );
        let has_memory_word = has_any_word(&lower, &["skill", "skills"]);

        if has_memory_term || has_memory_word {
            categories.insert(ToolCategory::Memory);
        }

        // 8. MiniPower Autonomous Methodology & Verification Intent
        let has_power_term = contains_any(
            &lower,
            &[
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
                "red flag",
                "worktree",
                "compliance",
                "acceptance criteria",
                "end-to-end",
            ],
        );
        let has_engineering_word =
            has_any_word(&lower, &["implement", "refactor", "feature", "milestone"]);

        if has_power_term || has_engineering_word {
            categories.insert(ToolCategory::MiniPower);
            categories.insert(ToolCategory::Memory);
        }

        // 9. MiniBlocks UI Component & Design Warehouse Intent
        let has_block_term = contains_any(
            &lower,
            &[
                "block",
                "component",
                "palette",
                "gradient",
                "navbar",
                "hero",
                "ui design",
                "design token",
                "landing",
                "dashboard",
                "sidebar",
                "pricing",
                "modal",
                "dialog",
                "tailwind",
                "dark mode",
                "light mode",
                "responsive",
            ],
        );
        let has_ui_word = has_any_word(
            &lower,
            &[
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
        );

        if has_block_term || has_ui_word {
            categories.insert(ToolCategory::Blocks);
            categories.insert(ToolCategory::MiniKit);
        }

        // 10. MiniTask Process Vault Intent
        let has_process_term = contains_any(
            &lower,
            &[
                "dev server",
                "minitask",
                "task manager",
                "background process",
                "background task",
                "scheduled task",
                "one-shot timer",
                "periodic check",
                "listen on port",
            ],
        ) || crate::utils::has_word(&lower, "daemon")
            || crate::utils::has_word(&lower, "vite")
            || lower.starts_with("serve")
            || lower.contains(" serve ")
            || lower.contains("port ");

        let has_server_action = crate::utils::has_word(&lower, "server")
            && has_any_word(
                &lower,
                &[
                    "start", "run", "launch", "serve", "status", "logs", "kill", "stop", "restart",
                ],
            );

        let has_resource_telemetry = contains_any(
            &lower,
            &["resource", "telemetry", "memory usage", "ram", "cpu"],
        );

        let has_status_inspection = contains_any(
            &lower,
            &[
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
            ],
        ) || (contains_any(&lower, &["how are", "what are"])
            && contains_any(&lower, &["tasks", "background processes"]));

        let has_teardown_command = matches!(lower.trim(), "stop" | "kill" | "stop it" | "kill it")
            || contains_any(&lower, &["stop that", "kill that", "manage task"]);

        if has_process_term
            || has_server_action
            || has_resource_telemetry
            || has_status_inspection
            || has_teardown_command
        {
            categories.insert(ToolCategory::Dev);
            categories.insert(ToolCategory::Exec);
        }

        categories
    }

    /// Detects relevant MCP server names from user prompt text and available servers.
    ///
    /// Matches explicit server names as tokens or substrings, as well as common domain keywords:
    /// - Figma: "figma", "frame", "component", "canvas", "design system"
    /// - GitHub: "github", "gh", "issue", "pull request", "pr"
    /// - Database / Postgres / MySQL: "postgres", "mysql", "sqlite", "database", "query", "sql"
    /// - Docker: "docker", "container", "compose", "image"
    #[must_use]
    pub fn detect_mcp_servers<I, S>(prompt: &str, available_servers: I) -> HashSet<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut matched = HashSet::new();
        let lower = prompt.to_ascii_lowercase();

        for server in available_servers {
            let s_ref = server.as_ref();
            let s_lower = s_ref.to_ascii_lowercase();
            // 1. Direct name match in prompt
            if lower.contains(&s_lower) {
                matched.insert(s_ref.to_string());
                continue;
            }

            // 2. Domain keyword matching
            match s_lower.as_str() {
                "figma" => {
                    if contains_any(
                        &lower,
                        &["frame", "component", "canvas", "design system", "ui design"],
                    ) {
                        matched.insert(s_ref.to_string());
                    }
                }
                "github" | "gh" => {
                    if contains_any(&lower, &["pull request", " pr ", "issue", "repo"]) {
                        matched.insert(s_ref.to_string());
                    }
                }
                "postgres" | "mysql" | "sqlite" | "database" | "db" | "sql" => {
                    if contains_any(&lower, &["sql", "query", "database", "migration", "schema"]) {
                        matched.insert(s_ref.to_string());
                    }
                }
                "docker" => {
                    if contains_any(&lower, &["container", "dockerfile", "compose"]) {
                        matched.insert(s_ref.to_string());
                    }
                }
                _ => {}
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
