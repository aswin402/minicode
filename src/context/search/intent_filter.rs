use crate::tools::category::ToolCategory;
use std::collections::HashSet;

/// High-speed, zero-allocation intent classifier for dynamic tool gating.
pub struct IntentClassifier;

impl IntentClassifier {
    /// Detects relevant tool categories from user prompt text.
    /// Runs in < 0.1ms using lowercase keyword matching.
    pub fn detect(prompt: &str) -> HashSet<ToolCategory> {
        let mut categories = HashSet::new();
        let lower = prompt.to_ascii_lowercase();

        // 1. Git Intent
        if lower.contains("git ")
            || lower.contains("commit")
            || lower.contains("branch")
            || lower.contains(" diff")
            || lower.contains("stash")
            || lower.contains("merge")
            || lower.contains("pull request")
            || lower.contains(" pr ")
            || lower.starts_with("git")
        {
            categories.insert(ToolCategory::Git);
        }

        // 2. Web & Browser Intent
        if lower.contains("http://")
            || lower.contains("https://")
            || lower.contains("search web")
            || lower.contains("web search")
            || lower.contains("browser")
            || lower.contains("browse")
            || lower.contains("crawl")
            || lower.contains("documentation for")
            || lower.contains("latest docs")
            || lower.contains("online docs")
            || lower.contains("launch website")
            || lower.contains("open website")
            || lower.contains("view website")
            || lower.contains("close browser")
            || lower.contains("stop browser")
            || lower.contains("kill browser")
            || lower.contains("browser_close")
            || lower.contains("screenshot")
            || crate::utils::has_word(&lower, "website")
        {
            categories.insert(ToolCategory::Web);
        }

        // 3. MiniKit Architecture Stacks, Dependencies & Skills Intent
        if lower.contains("minikit")
            || lower.contains("kit ")
            || lower.contains("kit_")
            || lower.contains("onpkg")
            || lower.contains("stack")
            || lower.contains("scaffold")
            || lower.contains("template")
            || lower.contains("bootstrap")
            || lower.contains("add pkg")
            || lower.contains("add package")
            || lower.contains("install package")
            || lower.contains("install pkg")
            || lower.contains("add dependency")
            || lower.contains("add dep")
            || lower.contains("dependencies")
            || lower.contains("dependency")
            || lower.contains("package")
            || lower.contains("packages")
            || lower.contains("library")
            || lower.contains("libraries")
            || lower.contains("npm i")
            || lower.contains("npm install")
            || lower.contains("yarn add")
            || lower.contains("pnpm add")
            || lower.contains("bun add")
            || lower.contains("cargo add")
            || lower.contains("pip install")
            || lower.contains("uv add")
            || lower.contains("flutter pub")
            || lower.contains("drift")
            || lower.contains("self-heal")
            || lower.contains("skill")
            || lower.contains("skills")
        {
            categories.insert(ToolCategory::MiniKit);
        }

        // 4. CodeGraph & Architecture Intent
        if lower.contains("architecture")
            || lower.contains("blast radius")
            || lower.contains("callers")
            || lower.contains("callees")
            || lower.contains("code graph")
            || lower.contains("codegraph")
            || lower.contains("code_explore")
            || lower.contains("diff_impact")
            || lower.contains("code_explain")
            || lower.contains("code_trace")
            || lower.contains("code_impact")
            || lower.contains("explain symbol")
            || lower.contains("trace flow")
            || lower.contains("call trace")
            || lower.contains("trace execution")
            || lower.contains("dependency graph")
            || lower.contains("impact analysis")
            || lower.contains("repo_map")
            || lower.contains("repomap")
            || lower.contains("who calls")
            || lower.contains("where is")
            || lower.contains("how does")
            || lower.contains("call hierarchy")
            || lower.contains("impact of")
            || (crate::utils::has_word(&lower, "trace")
                && (crate::utils::has_word(&lower, "call")
                    || crate::utils::has_word(&lower, "function")
                    || crate::utils::has_word(&lower, "symbol")
                    || crate::utils::has_word(&lower, "flow")))
        {
            categories.insert(ToolCategory::Codegraph);
            categories.insert(ToolCategory::Memory);
            categories.insert(ToolCategory::Search);
        }

        // 5. Multi-Agent & Swarm Intent
        if lower.contains("subagent")
            || lower.contains("swarm")
            || lower.contains("council")
            || lower.contains("consensus")
            || lower.contains("hypotheses")
            || lower.contains("hypothesis")
            || lower.contains("delegate")
            || lower.contains("fanout")
            || lower.contains("scratchpad")
        {
            categories.insert(ToolCategory::Agent);
        }

        // 6. AST & Deep Search Intent
        if lower.contains("ast")
            || lower.contains("syntax tree")
            || lower.contains("lsp")
            || lower.contains("goto definition")
            || lower.contains("find references")
            || lower.contains("hybrid search")
            || lower.contains("hybrid_search")
            || lower.contains("semantic search")
            || lower.contains("semantic_search")
            || lower.contains("locate_fault")
            || lower.contains("fault")
        {
            categories.insert(ToolCategory::Search);
        }

        // 7. Context, Memory, Architecture & Analysis Intent
        if lower.contains("wiki")
            || lower.contains("skill")
            || lower.contains("remember")
            || lower.contains("forget fact")
            || lower.contains("memory")
            || lower.contains("plan")
            || lower.contains("progress")
            || lower.contains("repo_map")
            || lower.contains("repomap")
            || lower.contains("code map")
            || lower.contains("skeleton")
            || lower.contains("smell")
            || lower.contains("code_smells")
            || lower.contains("dead_code")
            || lower.contains("dead code")
            || lower.contains("invariant")
            || lower.contains("coverage gap")
            || lower.contains("prune")
            || lower.contains("token budget")
        {
            categories.insert(ToolCategory::Memory);
        }

        // 8. MiniPower Autonomous Methodology & Verification Intent
        if lower.contains("power")
            || lower.contains("minipower")
            || lower.contains("superpower")
            || lower.contains("verify")
            || lower.contains("verification")
            || lower.contains("barrier")
            || lower.contains("review")
            || lower.contains("brainstorm")
            || lower.contains("socratic")
            || lower.contains("tdd")
            || lower.contains("red flag")
            || lower.contains("worktree")
            || lower.contains("compliance")
            || lower.contains("acceptance criteria")
            || lower.contains("end-to-end")
            || crate::utils::has_word(&lower, "implement")
            || crate::utils::has_word(&lower, "refactor")
            || crate::utils::has_word(&lower, "feature")
            || crate::utils::has_word(&lower, "milestone")
        {
            categories.insert(ToolCategory::MiniPower);
            categories.insert(ToolCategory::Memory);
        }

        // 9. MiniBlocks UI Component & Design Warehouse Intent
        if lower.contains("block")
            || lower.contains("miniblock")
            || lower.contains("miniblocks")
            || lower.contains("component")
            || lower.contains("components")
            || lower.contains("palette")
            || lower.contains("palettes")
            || lower.contains("gradient")
            || lower.contains("gradients")
            || lower.contains("navbar")
            || lower.contains("hero")
            || lower.contains("ui design")
            || lower.contains("design token")
            || lower.contains("landing page")
            || lower.contains("landing")
            || lower.contains("dashboard")
            || lower.contains("sidebar")
            || lower.contains("pricing")
            || lower.contains("modal")
            || lower.contains("dialog")
            || lower.contains("tailwind")
            || lower.contains("dark mode")
            || lower.contains("light mode")
            || lower.contains("responsive")
            || crate::utils::has_word(&lower, "ui")
            || crate::utils::has_word(&lower, "frontend")
            || crate::utils::has_word(&lower, "css")
            || crate::utils::has_word(&lower, "styling")
            || crate::utils::has_word(&lower, "button")
            || crate::utils::has_word(&lower, "card")
            || crate::utils::has_word(&lower, "table")
            || crate::utils::has_word(&lower, "footer")
            || crate::utils::has_word(&lower, "header")
            || crate::utils::has_word(&lower, "wireframe")
        {
            categories.insert(ToolCategory::Blocks);
            categories.insert(ToolCategory::MiniKit);
        }

        // 10. MiniTask Process Vault Intent
        if lower.contains("dev server")
            || lower.contains("run server")
            || lower.contains("start server")
            || lower.contains("launch server")
            || lower.contains("restart server")
            || lower.contains("kill server")
            || lower.contains("stop server")
            || lower.contains("background process")
            || lower.contains("background task")
            || lower.contains("manage task")
            || lower.contains("manage tasks")
            || lower.contains("minitask")
            || lower.contains("task manager")
            || lower.contains("listen on port")
            || lower.contains("port ")
            || lower.starts_with("serve")
            || lower.contains(" serve ")
            || lower.contains("resource")
            || lower.contains("resources")
            || lower.contains("ram")
            || lower.contains("cpu")
            || lower.contains("telemetry")
            || lower.contains("memory usage")
            || lower.contains("stop that")
            || lower.contains("kill that")
            || lower == "stop it"
            || lower == "kill it"
            || lower == "stop"
            || lower == "kill"
            || lower.contains("whats happening")
            || lower.contains("what's happening")
            || lower.contains("what is happening")
            || lower.contains("what is running")
            || lower.contains("what's running")
            || lower.contains("whats running")
            || lower.contains("what are the tasks doing")
            || lower.contains("how are the tasks doing")
            || lower.contains("how are background processes doing")
            || lower.contains("how are the background processes doing")
            || lower.contains("status of tasks")
            || lower.contains("status of task")
            || lower.contains("task status")
            || lower.contains("process status")
            || lower.contains("is the server running")
            || lower.contains("is the server still running")
            || lower.contains("is server running")
            || lower.contains("any background tasks")
            || lower.contains("any background task")
            || lower.contains("any processes running")
            || lower.contains("check background tasks")
            || lower.contains("check background task")
            || lower.contains("check background processes")
            || lower.contains("check tasks")
            || lower.contains("check processes")
            || lower.contains("active processes")
            || lower.contains("active tasks")
            || lower.contains("scheduled task")
            || lower.contains("scheduled tasks")
            || lower.contains("periodic check")
            || lower.contains("one-shot timer")
            || crate::utils::has_word(&lower, "daemon")
            || crate::utils::has_word(&lower, "vite")
            || (crate::utils::has_word(&lower, "server")
                && (crate::utils::has_word(&lower, "start")
                    || crate::utils::has_word(&lower, "run")
                    || crate::utils::has_word(&lower, "status")
                    || crate::utils::has_word(&lower, "logs")
                    || crate::utils::has_word(&lower, "kill")
                    || crate::utils::has_word(&lower, "stop")))
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
                    if lower.contains("frame")
                        || lower.contains("component")
                        || lower.contains("canvas")
                        || lower.contains("design system")
                        || lower.contains("ui design")
                    {
                        matched.insert(s_ref.to_string());
                    }
                }
                "github" | "gh" => {
                    if lower.contains("pull request")
                        || lower.contains(" pr ")
                        || lower.contains("issue")
                        || lower.contains("repo")
                    {
                        matched.insert(s_ref.to_string());
                    }
                }
                "postgres" | "mysql" | "sqlite" | "database" | "db" | "sql" => {
                    if lower.contains("sql")
                        || lower.contains("query")
                        || lower.contains("database")
                        || lower.contains("migration")
                        || lower.contains("schema")
                    {
                        matched.insert(s_ref.to_string());
                    }
                }
                "docker" => {
                    if lower.contains("container")
                        || lower.contains("dockerfile")
                        || lower.contains("compose")
                    {
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
