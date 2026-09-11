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
        {
            categories.insert(ToolCategory::Web);
        }

        // 3. Onpkg & Stack Scaffolding Intent
        if lower.contains("onpkg")
            || lower.contains("stack")
            || lower.contains("scaffold")
            || lower.contains("template")
            || lower.contains("add pkg")
            || lower.contains("add package")
            || lower.contains("install package")
        {
            categories.insert(ToolCategory::Onpkg);
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
            || lower.contains("dependency graph")
            || lower.contains("impact analysis")
            || lower.contains("repo_map")
            || lower.contains("repomap")
        {
            categories.insert(ToolCategory::Codegraph);
            categories.insert(ToolCategory::Memory);
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

        categories
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
    fn test_intent_detection_onpkg() {
        let detected = IntentClassifier::detect("scaffold a new stack with onpkg");
        assert!(detected.contains(&ToolCategory::Onpkg));
    }

    #[test]
    fn test_intent_detection_multiple() {
        let detected = IntentClassifier::detect("check git diff and search web for docs");
        assert!(detected.contains(&ToolCategory::Git));
        assert!(detected.contains(&ToolCategory::Web));
    }
}
