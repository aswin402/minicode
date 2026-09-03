use crate::agent::provider::ToolSchema;
use crate::tools::registry;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;

/// Use-case domain bundles for minicode tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Files,
    Exec,
    Search,
    Git,
    Web,
    Onpkg,
    Codegraph,
    Agent,
    Memory,
}

impl ToolCategory {
    pub const ALL: [ToolCategory; 9] = [
        ToolCategory::Files,
        ToolCategory::Exec,
        ToolCategory::Search,
        ToolCategory::Git,
        ToolCategory::Web,
        ToolCategory::Onpkg,
        ToolCategory::Codegraph,
        ToolCategory::Agent,
        ToolCategory::Memory,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Exec => "exec",
            Self::Search => "search",
            Self::Git => "git",
            Self::Web => "web",
            Self::Onpkg => "onpkg",
            Self::Codegraph => "codegraph",
            Self::Agent => "agent",
            Self::Memory => "memory",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Files => "Filesystem manipulation (read_file, patch_file, write_file)",
            Self::Exec => "Terminal command execution (exec_cmd)",
            Self::Search => "Codebase search & symbols (grep_search, locate_symbol, hybrid_search, ast_query)",
            Self::Git => "Git version control (git_status, git_diff, git_commit, git_branch, git_log)",
            Self::Web => "Web search & browser automation (search_web, fetch_or_browse, browser_navigate)",
            Self::Onpkg => "Onpkg stack scaffolding & packages (onpkg_stack_list, onpkg_stack_add, onpkg_pkg_add)",
            Self::Codegraph => "CodeGraph architecture & blast radius (code_explore, diff_impact, blast_radius)",
            Self::Agent => "Multi-agent coordination & hypotheses (dispatch_subagent, explore_hypotheses)",
            Self::Memory => "Progressive memory, planning & skills (create_plan, update_progress, wiki_write)",
        }
    }

    /// Returns the complete list of tool schemas belonging to this category.
    pub fn get_schemas(&self) -> Vec<ToolSchema> {
        match self {
            Self::Files => registry::fs_tools::get_schemas(),
            Self::Exec => registry::exec_tools::get_schemas(),
            Self::Search => registry::search_tools::get_schemas(),
            Self::Git => registry::git_tools::get_schemas(),
            Self::Web => registry::web_tools::get_schemas(),
            Self::Onpkg => registry::onpkg_tools::get_schemas(),
            Self::Codegraph => registry::explore_tools::get_schemas(),
            Self::Agent => registry::agent_tools::get_schemas(),
            Self::Memory => registry::context_tools::get_schemas(),
        }
    }
}

impl FromStr for ToolCategory {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().trim() {
            "files" | "file" | "fs" => Ok(Self::Files),
            "exec" | "cmd" | "terminal" | "shell" => Ok(Self::Exec),
            "search" | "find" | "ast" | "symbol" => Ok(Self::Search),
            "git" | "vcs" | "diff" => Ok(Self::Git),
            "web" | "browser" | "crawl" | "internet" => Ok(Self::Web),
            "onpkg" | "stack" | "pkg" | "package" => Ok(Self::Onpkg),
            "codegraph" | "graph" | "explore" | "architecture" => Ok(Self::Codegraph),
            "agent" | "agents" | "swarm" | "subagent" => Ok(Self::Agent),
            "memory" | "plan" | "wiki" | "skill" | "skills" => Ok(Self::Memory),
            other => Err(format!(
                "Unknown tool category '{}'. Available: files, exec, search, git, web, onpkg, codegraph, agent, memory, all",
                other
            )),
        }
    }
}

/// JSON Schema for the `activate_tools` dynamic meta-tool.
pub fn activate_tools_schema() -> ToolSchema {
    ToolSchema {
        name: "activate_tools".to_string(),
        description: "Dynamically activate a specialized tool category into your active toolset for this turn. Available categories: 'git', 'web', 'codegraph', 'onpkg', 'agent', 'search', 'memory', 'files', 'exec', or 'all'.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "The category of tools to activate: 'git', 'web', 'codegraph', 'onpkg', 'agent', 'search', 'memory', 'files', 'exec', or 'all'",
                    "enum": ["git", "web", "codegraph", "onpkg", "agent", "search", "memory", "files", "exec", "all"]
                },
                "reason": {
                    "type": "string",
                    "description": "Brief explanation of why these tools are needed for the current task"
                }
            },
            "required": ["category"]
        }),
    }
}

/// Returns the minimal, highly-optimized Core tool schemas (~9 tools) always exposed in Dynamic mode.
pub fn get_core_schemas() -> Vec<ToolSchema> {
    let mut core = Vec::with_capacity(10);
    // 1. Files (read_file, patch_file, write_file)
    core.extend(registry::fs_tools::get_schemas());

    // 2. Exec (exec_cmd)
    core.extend(registry::exec_tools::get_schemas());

    // 3. Search baseline (grep_search, locate_symbol)
    for s in registry::search_tools::get_schemas() {
        if s.name == "grep_search" || s.name == "locate_symbol" {
            core.push(s);
        }
    }

    // 4. Memory baseline (create_plan, update_progress)
    for s in registry::context_tools::get_schemas() {
        if s.name == "create_plan" || s.name == "update_progress" {
            core.push(s);
        }
    }

    // 5. Meta-Tool
    core.push(activate_tools_schema());

    core
}

/// Assembles active schemas for an agent turn based on configuration mode, prompt intent, and dynamic activations.
pub fn assemble_active_tools(
    mode: crate::config::ToolFilterMode,
    user_prompt: &str,
    dynamic_categories: &HashSet<ToolCategory>,
) -> Vec<ToolSchema> {
    match mode {
        crate::config::ToolFilterMode::Full => {
            let mut all = Vec::with_capacity(crate::constants::TOTAL_TOOL_COUNT + 1);
            for cat in &ToolCategory::ALL {
                all.extend(cat.get_schemas());
            }
            all.push(activate_tools_schema());
            all
        }
        crate::config::ToolFilterMode::CoreOnly => get_core_schemas(),
        crate::config::ToolFilterMode::Dynamic => {
            let mut schemas = get_core_schemas();
            let mut included_names: HashSet<String> =
                schemas.iter().map(|s| s.name.clone()).collect();

            // Detect categories from prompt intent
            let detected = crate::context::intent_filter::IntentClassifier::detect(user_prompt);

            // Merge detected intent + explicitly activated categories
            let mut active_cats = dynamic_categories.clone();
            for cat in detected {
                active_cats.insert(cat);
            }

            for cat in active_cats {
                for schema in cat.get_schemas() {
                    if !included_names.contains(&schema.name) {
                        included_names.insert(schema.name.clone());
                        schemas.push(schema);
                    }
                }
            }

            schemas
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_schemas_count() {
        let core = get_core_schemas();
        assert!(core.len() >= 8 && core.len() <= 10);
        let names: Vec<&str> = core.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"patch_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"exec_cmd"));
        assert!(names.contains(&"grep_search"));
        assert!(names.contains(&"locate_symbol"));
        assert!(names.contains(&"create_plan"));
        assert!(names.contains(&"update_progress"));
        assert!(names.contains(&"activate_tools"));
    }

    #[test]
    fn test_category_parsing() {
        assert_eq!("git".parse::<ToolCategory>().unwrap(), ToolCategory::Git);
        assert_eq!("web".parse::<ToolCategory>().unwrap(), ToolCategory::Web);
        assert_eq!(
            "codegraph".parse::<ToolCategory>().unwrap(),
            ToolCategory::Codegraph
        );
        assert_eq!(
            "onpkg".parse::<ToolCategory>().unwrap(),
            ToolCategory::Onpkg
        );
        assert_eq!(
            "agent".parse::<ToolCategory>().unwrap(),
            ToolCategory::Agent
        );
    }
}
