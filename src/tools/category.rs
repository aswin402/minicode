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
    #[serde(alias = "onpkg")]
    MiniKit,
    Codegraph,
    Agent,
    Memory,
    #[serde(alias = "power")]
    MiniPower,
    #[serde(alias = "miniblocks")]
    Blocks,
    #[serde(alias = "minidev")]
    Dev,
    #[serde(alias = "minivault")]
    Vault,
}

impl ToolCategory {
    #[allow(non_upper_case_globals, dead_code)]
    pub const Onpkg: ToolCategory = ToolCategory::MiniKit;
    #[allow(non_upper_case_globals, dead_code)]
    pub const Power: ToolCategory = ToolCategory::MiniPower;

    pub const ALL: [ToolCategory; 13] = [
        ToolCategory::Files,
        ToolCategory::Exec,
        ToolCategory::Search,
        ToolCategory::Git,
        ToolCategory::Web,
        ToolCategory::MiniKit,
        ToolCategory::Codegraph,
        ToolCategory::Agent,
        ToolCategory::Memory,
        ToolCategory::MiniPower,
        ToolCategory::Blocks,
        ToolCategory::Dev,
        ToolCategory::Vault,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Exec => "exec",
            Self::Search => "search",
            Self::Git => "git",
            Self::Web => "web",
            Self::MiniKit => "minikit",
            Self::Codegraph => "codegraph",
            Self::Agent => "agent",
            Self::Memory => "memory",
            Self::MiniPower => "minipower",
            Self::Blocks => "blocks",
            Self::Dev => "dev",
            Self::Vault => "vault",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Files => "Filesystem manipulation (read_file, patch_file, write_file, replace_in_files)",
            Self::Exec => "Terminal command execution (exec_cmd)",
            Self::Search => "Codebase search & AST patterns (grep_search, locate_symbol, hybrid_search, ast_query, ast_grep_search, ast_grep_rewrite)",
            Self::Git => "Git version control (git_status, git_diff, git_commit, git_branch, git_log)",
            Self::Web => "Web search & browser automation (search_web, webfetch, browser_navigate)",
            Self::MiniKit => "MiniKit stack scaffolding, packages & skills (kit_stack_add, kit_add, kit_sync)",
            Self::Codegraph => "CodeGraph architecture, AST refactoring & blast radius (code_explore, diff_impact, code_impact, ast_refactor, impact_analysis, repo_map)",
            Self::Agent => "Multi-agent coordination & hypotheses (dispatch_subagent, explore_hypotheses)",
            Self::Memory => "Progressive memory, planning, DOX hierarchy, OKF v0.2 knowledge catalog & skills (create_plan, update_progress, dox_tree, wiki_write, okf_attest, okf_bundle)",
            Self::MiniPower => "MiniPower methodology, verification barrier, session forensics & worktree tasks (power_brainstorm, power_plan, power_review, power_verify, power_worktree_task, power_diagnose)",
            Self::Blocks => "MiniBlocks UI component & design token warehouse (block_search, block_get, block_insert, block_save, block_update, block_delete, block_palettes, block_gradients, block_scaffold, block_stats)",
            Self::Dev => "Unified task manager, process vault, development servers & daemons (minitask)",
            Self::Vault => "MiniVault multi-tier agent skills warehouse, SkillSpector security scanner, bundles & lifecycle (vault_search, vault_show, vault_scan, vault_load, vault_unload, vault_create, vault_update, vault_delete, vault_import_url, vault_bundle_list, vault_bundle_load, vault_bundle_create)",
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
            Self::MiniKit => registry::minikit_tools::get_schemas(),
            Self::Codegraph => registry::explore_tools::get_schemas(),
            Self::Agent => registry::agent_tools::get_schemas(),
            Self::Memory => registry::context_tools::get_schemas(),
            Self::MiniPower => registry::minipower_tools::get_schemas(),
            Self::Blocks => registry::block_tools::get_schemas(),
            Self::Dev => registry::dev_tools::get_schemas(),
            Self::Vault => registry::vault_tools::get_schemas(),
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
            "kit" | "minikit" | "onpkg" | "stack" | "pkg" | "package" | "dep" | "dependencies" => Ok(Self::MiniKit),
            "codegraph" | "graph" | "explore" | "architecture" => Ok(Self::Codegraph),
            "agent" | "agents" | "swarm" | "subagent" => Ok(Self::Agent),
            "memory" | "plan" | "wiki" | "okf" | "attest" | "catalog" | "dox" => Ok(Self::Memory),
            "power" | "minipower" | "superpower" | "superpowers" | "verify" | "verification" | "review" | "brainstorm" | "worktree" | "tdd" => Ok(Self::MiniPower),
            "blocks" | "block" | "miniblocks" | "component" | "components" | "palette" | "palettes" => Ok(Self::Blocks),
            "dev" | "minidev" | "server" | "servers" | "daemon" | "orchestrator" => Ok(Self::Dev),
            "vault" | "minivault" | "skill" | "skills" | "bundle" | "bundles" => Ok(Self::Vault),
            other => Err(format!(
                "Unknown tool category '{}'. Available: files, exec, search, git, web, kit, codegraph, agent, memory, power, blocks, dev, vault, all",
                other
            )),
        }
    }
}

/// JSON Schema for the `activate_tools` dynamic meta-tool.
pub fn activate_tools_schema() -> ToolSchema {
    activate_tools_schema_with_mcp(&[])
}

/// JSON Schema for the `activate_tools` dynamic meta-tool with connected MCP servers advertised.
pub fn activate_tools_schema_with_mcp(mcp_servers: &[(&str, usize)]) -> ToolSchema {
    let mut desc = "Dynamically activate a specialized tool category, specific individual tool(s) (`select:<tool_name>` or keyword search), or an MCP server into your active toolset for this turn. Available native categories: 'git', 'web', 'codegraph', 'kit' (MiniKit stacks & packages), 'power' (MiniPower methodology & verification), 'blocks' (MiniBlocks UI warehouse), 'dev' (MiniDev runtime & servers), 'vault' (MiniVault skills & bundles), 'agent', 'search', 'memory', 'files', 'exec', or 'all'.".to_string();

    if !mcp_servers.is_empty() {
        desc.push_str(" Connected MCP servers: ");
        let entries: Vec<String> = mcp_servers
            .iter()
            .map(|(name, count)| format!("'{}' ({} tools)", name, count))
            .collect();
        desc.push_str(&entries.join(", "));
        desc.push('.');
    }

    ToolSchema {
        name: "activate_tools".to_string(),
        description: desc,
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Category name ('git', 'web', 'codegraph', 'minikit', 'power', 'blocks', 'dev', 'vault', 'agent', 'search', 'memory', 'files', 'exec', 'all', 'mcp'), MCP server ('mcp:<name>'), specific tool ('select:<tool_name>'), or keyword search query"
                },
                "query": {
                    "type": "string",
                    "description": "Optional keyword query or comma-separated tool names to selectively activate only matching individual tool schemas without loading an entire category"
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

/// Returns the minimal, highly-optimized Core tool schemas (`CORE_ACTIVE_TOOL_NAMES`: 10 tools)
/// always exposed upfront in `Dynamic` and `CoreOnly` modes.
pub fn get_core_schemas() -> Vec<ToolSchema> {
    let mut all = collect_all_schemas(&std::collections::HashMap::new());
    all.push(activate_tools_schema());
    let mut core = Vec::with_capacity(crate::constants::CORE_ACTIVE_TOOL_NAMES.len());
    for &core_name in crate::constants::CORE_ACTIVE_TOOL_NAMES {
        if let Some(pos) = all.iter().position(|s| s.name == core_name) {
            core.push(all.swap_remove(pos));
        }
    }
    core
}

/// Formats a deterministic `<tool_registry_telemetry>` and `<available_deferred_tools>` block
/// modeled after Claude Code (`ToolSearch`) and Prime Agent (Compact Summary Pool).
/// Provides exact active, deferred, and total tool counts alongside per-category tool names
/// so the LLM never manually counts schemas or hallucinates unregistered tool names.
#[must_use]
pub fn format_tool_registry_telemetry_block(
    active_tools: &[ToolSchema],
    mcp_tools_by_server: &std::collections::HashMap<String, Vec<ToolSchema>>,
) -> String {
    format_tool_registry_telemetry_block_with_detail(active_tools, mcp_tools_by_server, true)
}

/// Formats `<tool_registry_telemetry>` with optional expansion of `<available_deferred_tools>`.
/// On zero-energy conversational prompts (where `include_deferred_catalog == false`), the
/// top-level Registry Summary and Active Tools list are preserved while the 212 deferred tool
/// names are omitted to conserve input tokens.
#[must_use]
pub fn format_tool_registry_telemetry_block_with_detail(
    active_tools: &[ToolSchema],
    mcp_tools_by_server: &std::collections::HashMap<String, Vec<ToolSchema>>,
    include_deferred_catalog: bool,
) -> String {
    let active_names_set: HashSet<&str> = active_tools.iter().map(|s| s.name.as_str()).collect();
    let mut sorted_active: Vec<&str> = active_tools.iter().map(|s| s.name.as_str()).collect();
    sorted_active.sort_unstable();

    let builtin_domain_total = crate::constants::TOTAL_TOOL_COUNT;
    let mcp_total: usize = mcp_tools_by_server.values().map(|v| v.len()).sum();
    let has_meta = active_names_set.contains("activate_tools");
    let meta_count = usize::from(has_meta);
    let total_registered_with_meta = builtin_domain_total + meta_count + mcp_total;
    let active_count = active_tools.len();
    let deferred_count = total_registered_with_meta.saturating_sub(active_count);

    let mut out = String::from("  <tool_registry_telemetry>\n");
    out.push_str(&format!(
        "    Registry Summary: {} Active Tool Schemas in Context | {} Deferred Tools (On-Demand via `activate_tools`) | {} Built-in Domain Tools + {} Meta-Tool (`activate_tools`){} = {} Total Tools\n",
        active_count,
        deferred_count,
        builtin_domain_total,
        meta_count,
        if mcp_total > 0 {
            format!(" + {} MCP Tools", mcp_total)
        } else {
            String::new()
        },
        total_registered_with_meta
    ));
    let active_formatted: Vec<String> = sorted_active.iter().map(|n| format!("`{}`", n)).collect();
    out.push_str(&format!(
        "    Active Tools ({}): {}\n",
        active_count,
        active_formatted.join(", ")
    ));
    if !include_deferred_catalog {
        out.push_str("  </tool_registry_telemetry>");
        return out;
    }
    out.push_str(
        "    <available_deferred_tools hint=\"Deferred tools are NOT sent as full JSON schemas until needed. Activate any category (`activate_tools(category=\\\"...\\\")`) or specific tool (`activate_tools(category=\\\"select:tool_a,tool_b\\\")`), or invoke directly by name\">\n",
    );

    for cat in &ToolCategory::ALL {
        let schemas = cat.get_schemas();
        let cat_total = schemas.len();
        let mut active_in_cat: Vec<String> = Vec::new();
        let mut deferred_names: Vec<String> = Vec::new();
        for s in &schemas {
            if active_names_set.contains(s.name.as_str()) {
                active_in_cat.push(format!("`{}`", s.name));
            } else {
                deferred_names.push(format!("`{}`", s.name));
            }
        }
        active_in_cat.sort_unstable();
        deferred_names.sort_unstable();
        let active_summary = if active_in_cat.is_empty() {
            "0 active".to_string()
        } else {
            format!(
                "{} active [{}]",
                active_in_cat.len(),
                active_in_cat.join(", ")
            )
        };
        if deferred_names.is_empty() {
            out.push_str(&format!(
                "      • {} ({} total, {}, 0 deferred): all active\n",
                cat.name(),
                cat_total,
                active_summary
            ));
        } else {
            out.push_str(&format!(
                "      • {} ({} total, {}, {} deferred): {}\n",
                cat.name(),
                cat_total,
                active_summary,
                deferred_names.len(),
                deferred_names.join(", ")
            ));
        }
    }

    if !mcp_tools_by_server.is_empty() {
        let mut server_names: Vec<&String> = mcp_tools_by_server.keys().collect();
        server_names.sort_unstable();
        for srv in server_names {
            if let Some(tools) = mcp_tools_by_server.get(srv) {
                let mut srv_active = 0usize;
                let mut srv_deferred: Vec<String> = Vec::new();
                for s in tools {
                    if active_names_set.contains(s.name.as_str()) {
                        srv_active += 1;
                    } else {
                        srv_deferred.push(format!("`{}`", s.name));
                    }
                }
                srv_deferred.sort_unstable();
                out.push_str(&format!(
                    "      • mcp:{} ({} total, {} active, {} deferred): {}\n",
                    srv,
                    tools.len(),
                    srv_active,
                    srv_deferred.len(),
                    if srv_deferred.is_empty() {
                        "all active".to_string()
                    } else {
                        srv_deferred.join(", ")
                    }
                ));
            }
        }
    }

    out.push_str("    </available_deferred_tools>\n");
    out.push_str("  </tool_registry_telemetry>");
    out
}

/// Backward-compatible wrapper assembling active tools without external MCP servers.
#[allow(dead_code)]
pub fn assemble_active_tools(
    mode: crate::config::ToolFilterMode,
    user_prompt: &str,
    dynamic_categories: &HashSet<ToolCategory>,
) -> Vec<ToolSchema> {
    assemble_active_tools_with_mcp(
        mode,
        user_prompt,
        dynamic_categories,
        &std::collections::HashMap::new(),
        &HashSet::new(),
    )
}

fn collect_all_schemas(
    mcp_tools_by_server: &std::collections::HashMap<String, Vec<ToolSchema>>,
) -> Vec<ToolSchema> {
    let mut all = Vec::with_capacity(crate::constants::TOTAL_TOOL_COUNT + 1);
    for cat in &ToolCategory::ALL {
        all.extend(cat.get_schemas());
    }
    for tools in mcp_tools_by_server.values() {
        all.extend(tools.iter().cloned());
    }
    all
}

/// Searches all built-in and MCP tool schemas by exact tool name (`select:tool_a,tool_b`)
/// or keyword query, returning up to 6 best-matching individual tool schemas.
pub fn search_individual_tools(
    query: &str,
    mcp_tools_by_server: &std::collections::HashMap<String, Vec<ToolSchema>>,
) -> Vec<ToolSchema> {
    let raw = query
        .trim()
        .strip_prefix("select:")
        .or_else(|| query.trim().strip_prefix("tool:"))
        .unwrap_or_else(|| query.trim());
    if raw.is_empty() {
        return Vec::new();
    }

    let all_schemas = collect_all_schemas(mcp_tools_by_server);
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    // 1. Check comma-separated exact tool names first
    for part in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let clean_part = part
            .strip_prefix("select:")
            .or_else(|| part.strip_prefix("tool:"))
            .unwrap_or(part)
            .trim();
        let lower_part = clean_part.to_ascii_lowercase();
        if let Some(exact) = all_schemas
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&lower_part))
        {
            if seen.insert(exact.name.clone()) {
                results.push(exact.clone());
            }
        }
    }
    if !results.is_empty() {
        return results;
    }

    let mut all_schemas = all_schemas;
    if raw.to_ascii_lowercase().starts_with("mcp_") {
        all_schemas.retain(|s| s.name.starts_with(crate::constants::MCP_TOOL_PREFIX));
    }

    // 2. Keyword & underscore-segment scoring across tool names and descriptions
    let tokens: Vec<String> = raw
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| {
            t.len() >= 2
                && !matches!(
                    t.as_str(),
                    "tool" | "tools" | "the" | "and" | "for" | "with" | "from" | "into"
                )
        })
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(usize, ToolSchema)> = Vec::new();
    for schema in all_schemas {
        let name_lower = schema.name.to_ascii_lowercase();
        let desc_lower = schema.description.to_ascii_lowercase();
        let mut score = 0usize;
        for tok in &tokens {
            if name_lower == *tok {
                score += 100;
            } else if name_lower.split('_').any(|seg| seg == tok) {
                score += 50;
            } else if name_lower.contains(tok.as_str()) {
                score += 30;
            }
            if desc_lower.contains(tok.as_str()) {
                score += 10;
            }
        }
        if score >= 20 || (tokens.len() == 1 && score >= 10) {
            scored.push((score, schema));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.into_iter().take(6).map(|(_, s)| s).collect()
}

/// Assembles active schemas for an agent turn based on configuration mode, prompt intent,
/// dynamically activated categories, and connected MCP servers.
pub fn assemble_active_tools_with_mcp(
    mode: crate::config::ToolFilterMode,
    user_prompt: &str,
    dynamic_categories: &HashSet<ToolCategory>,
    mcp_tools_by_server: &std::collections::HashMap<String, Vec<ToolSchema>>,
    dynamic_mcp_servers: &HashSet<String>,
) -> Vec<ToolSchema> {
    assemble_active_tools_full(
        mode,
        user_prompt,
        dynamic_categories,
        mcp_tools_by_server,
        dynamic_mcp_servers,
        &HashSet::new(),
    )
}

/// Assembles active schemas for an agent turn including individually selected/searched tools.
pub fn assemble_active_tools_full(
    mode: crate::config::ToolFilterMode,
    user_prompt: &str,
    dynamic_categories: &HashSet<ToolCategory>,
    mcp_tools_by_server: &std::collections::HashMap<String, Vec<ToolSchema>>,
    dynamic_mcp_servers: &HashSet<String>,
    dynamic_tool_names: &HashSet<String>,
) -> Vec<ToolSchema> {
    let mcp_server_info: Vec<(&str, usize)> = mcp_tools_by_server
        .iter()
        .map(|(k, v)| (k.as_str(), v.len()))
        .collect();

    let mut assembled = match mode {
        crate::config::ToolFilterMode::Full => {
            let mut all = collect_all_schemas(mcp_tools_by_server);
            all.push(activate_tools_schema_with_mcp(&mcp_server_info));
            all
        }
        crate::config::ToolFilterMode::ReadOnly => {
            let mut all = collect_all_schemas(mcp_tools_by_server);
            all.retain(|s| crate::tools::is_read_only(&s.name));
            all
        }
        crate::config::ToolFilterMode::Standard => {
            let mut all = collect_all_schemas(mcp_tools_by_server);
            all.retain(|s| s.name != "spawn_subagent" && s.name != "activate_tools");
            all
        }
        crate::config::ToolFilterMode::CoreOnly => {
            let mut core = get_core_schemas();
            if let Some(pos) = core.iter().position(|s| s.name == "activate_tools") {
                core[pos] = activate_tools_schema_with_mcp(&mcp_server_info);
            }
            core
        }
        crate::config::ToolFilterMode::Dynamic => {
            let mut schemas = get_core_schemas();
            if let Some(pos) = schemas.iter().position(|s| s.name == "activate_tools") {
                schemas[pos] = activate_tools_schema_with_mcp(&mcp_server_info);
            }
            let mut included_names: HashSet<String> =
                schemas.iter().map(|s| s.name.clone()).collect();

            // 1. Detect native categories from prompt intent
            let detected =
                crate::context::search::intent_filter::IntentClassifier::detect(user_prompt);

            // Merge detected intent + explicitly activated categories
            let mut active_cats = dynamic_categories.clone();
            active_cats.extend(detected);

            // Automatic Liveness Promotion:
            // If background tasks or a live browser session are running (and the user is not asking a
            // conversational meta-introspection question), ensure Dev and Web tools are available.
            let is_meta =
                crate::context::search::intent_filter::IntentClassifier::is_meta_introspection_query(
                    user_prompt,
                );
            let (active_task_count, _, _) =
                crate::dev::registry::get_global_dev_registry().get_telemetry_snapshot();
            let browser_active =
                crate::tools::browser::BrowserManager::is_live_engine_running_sync();
            if !is_meta && (active_task_count > 0 || browser_active) {
                active_cats.insert(ToolCategory::Dev);
                if browser_active {
                    active_cats.insert(ToolCategory::Web);
                }
            }

            // Swarm Worker Promotion:
            // If running inside a swarm worker process, ensure Agent tools (send_worker_message)
            // are unconditionally available for inter-worker coordination.
            if std::env::var("MINICODE_SWARM_DIR")
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
            {
                active_cats.insert(ToolCategory::Agent);
            }

            for cat in &active_cats {
                for schema in cat.get_schemas() {
                    if included_names.insert(schema.name.clone()) {
                        schemas.push(schema);
                    }
                }
            }
            if active_cats.contains(&ToolCategory::Codegraph) {
                for schema in registry::context_tools::graph::get_schemas() {
                    if included_names.insert(schema.name.clone()) {
                        schemas.push(schema);
                    }
                }
            }

            // 2. Dynamic MCP Server Gating
            let total_mcp_tools: usize = mcp_tools_by_server.values().map(|v| v.len()).sum();
            let activate_all_mcp = dynamic_mcp_servers.contains("all")
                || dynamic_mcp_servers.contains("mcp")
                || (total_mcp_tools > 0 && total_mcp_tools <= 4);

            if activate_all_mcp {
                for tools in mcp_tools_by_server.values() {
                    for schema in tools {
                        if included_names.insert(schema.name.clone()) {
                            schemas.push(schema.clone());
                        }
                    }
                }
            } else {
                let server_keys: Vec<&str> =
                    mcp_tools_by_server.keys().map(|k| k.as_str()).collect();
                let detected_servers =
                    crate::context::search::intent_filter::IntentClassifier::detect_mcp_servers(
                        user_prompt,
                        server_keys,
                    );

                for (server_name, tools) in mcp_tools_by_server {
                    let is_active = dynamic_mcp_servers.contains(server_name)
                        || dynamic_mcp_servers.contains(&format!("mcp:{}", server_name))
                        || detected_servers.contains(server_name);

                    if is_active {
                        for schema in tools {
                            if included_names.insert(schema.name.clone()) {
                                schemas.push(schema.clone());
                            }
                        }
                    }
                }
            }

            // 3. Individually activated tools (via `select:<tool>` or keyword search in `activate_tools`)
            if !dynamic_tool_names.is_empty() {
                for schema in collect_all_schemas(mcp_tools_by_server) {
                    if dynamic_tool_names.contains(&schema.name)
                        && included_names.insert(schema.name.clone())
                    {
                        schemas.push(schema);
                    }
                }
            }

            schemas
        }
    };

    crate::context::budget::headroom::CacheAligner::normalize_tool_schemas(&mut assembled);
    assembled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_schemas_count() {
        let core = get_core_schemas();
        assert_eq!(core.len(), crate::constants::CORE_ACTIVE_TOOL_NAMES.len());
        let names: Vec<&str> = core.iter().map(|s| s.name.as_str()).collect();
        for &expected in crate::constants::CORE_ACTIVE_TOOL_NAMES {
            assert!(
                names.contains(&expected),
                "Missing expected core tool '{}'",
                expected
            );
        }
    }

    #[test]
    fn test_format_tool_registry_telemetry_block() {
        let core = get_core_schemas();
        let empty_mcp = std::collections::HashMap::new();
        let block = format_tool_registry_telemetry_block(&core, &empty_mcp);
        assert!(block.contains("<tool_registry_telemetry>"));
        assert!(block.contains("10 Active Tool Schemas in Context"));
        assert!(block.contains(&format!(
            "{} Built-in Domain Tools",
            crate::constants::TOTAL_TOOL_COUNT
        )));
        assert!(block.contains("<available_deferred_tools"));
        assert!(block.contains("`code_explore`"));
        assert!(block.contains("`ast_grep_search`"));
        assert!(block.contains("`power_speckit`"));
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
            ToolCategory::MiniKit
        );
        assert_eq!(
            "minikit".parse::<ToolCategory>().unwrap(),
            ToolCategory::MiniKit
        );
        assert_eq!(
            "agent".parse::<ToolCategory>().unwrap(),
            ToolCategory::Agent
        );
        assert_eq!(
            "blocks".parse::<ToolCategory>().unwrap(),
            ToolCategory::Blocks
        );
        assert_eq!(
            "miniblocks".parse::<ToolCategory>().unwrap(),
            ToolCategory::Blocks
        );
    }

    #[test]
    fn test_search_individual_tools_and_selective_activation() {
        let empty_mcp = std::collections::HashMap::new();
        let exact = search_individual_tools("select:git_diff,ast_replace_node", &empty_mcp);
        let exact_names: Vec<&str> = exact.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(exact_names, vec!["git_diff", "ast_replace_node"]);

        let keyword = search_individual_tools("blast radius impact", &empty_mcp);
        assert!(!keyword.is_empty());
        assert!(keyword
            .iter()
            .any(|s| s.name == "code_impact" || s.name == "impact_analysis"));

        let mut dyn_tools = HashSet::new();
        dyn_tools.insert("code_impact".to_string());
        let assembled = assemble_active_tools_full(
            crate::config::ToolFilterMode::Dynamic,
            "say hello",
            &HashSet::new(),
            &empty_mcp,
            &HashSet::new(),
            &dyn_tools,
        );
        assert!(assembled.iter().any(|s| s.name == "code_impact"));
        // Did not load the entire Codegraph category (e.g. code_trace remains unloaded)
        assert!(!assembled.iter().any(|s| s.name == "code_trace"));
    }

    #[test]
    fn test_zero_phantom_tools_in_features_and_builtin_skills() {
        let mut valid_tools: HashSet<String> = crate::tools::ToolRegistry::get_tool_schemas()
            .into_iter()
            .map(|s| s.name)
            .collect();
        valid_tools.insert("activate_tools".to_string());

        for feature in crate::agent::features::AgentFeature::ALL {
            for &tool_name in feature.tools() {
                assert!(
                    valid_tools.contains(tool_name),
                    "AgentFeature::{:?} references unregistered phantom tool '{}'",
                    feature,
                    tool_name
                );
            }
        }

        for skill in crate::vault::builtin::get_all_builtin_skills() {
            for tool_name in &skill.frontmatter.allowed_tools {
                assert!(
                    valid_tools.contains(tool_name.as_str()),
                    "Builtin skill '{}' references unregistered phantom tool '{}' in allowed_tools",
                    skill.name,
                    tool_name
                );
            }
        }
    }
}
