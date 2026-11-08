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
            "ast_refactor",
            "scan_symbol",
            "delete_symbol",
            "find_unused",
            "move_symbol",
            "rename_file",
            "css_class_rename",
            "css_var_rename",
            "rename_symbol",
            "extract_function",
            "inline_variable",
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
            "unused symbols",
            "rename symbol",
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

/// Multi-dimensional information-theoretic signal vector extracted from a user prompt in O(N) time.
/// Drives proportional context allocation, progressive skill disclosure, and plan-continuation gating
/// without hardcoded greeting lists, regexes, or ad-hoc branching.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromptSignalVector {
    pub word_count: usize,
    pub domain_score: usize,
    pub action_score: usize,
    pub code_morphology_score: usize,
    pub plan_continuation_score: usize,
    pub is_meta_query: bool,
}

impl PromptSignalVector {
    /// Total actionable engineering signal energy $E(P) = S_{\text{domain}} + S_{\text{action}} + S_{\text{code}} + S_{\text{plan}}$.
    #[inline]
    #[must_use]
    pub const fn task_energy(&self) -> usize {
        self.domain_score
            .saturating_add(self.action_score)
            .saturating_add(self.code_morphology_score)
            .saturating_add(self.plan_continuation_score)
    }

    /// Returns `true` when the prompt carries positive engineering/task signal and is not a meta-introspection query.
    #[inline]
    #[must_use]
    pub const fn has_actionable_task_intent(&self) -> bool {
        !self.is_meta_query && self.task_energy() > 0
    }

    /// Returns `true` when the prompt exhibits lexical or semantic affinity with an active plan/todo.
    #[inline]
    #[must_use]
    pub const fn has_plan_continuation_affinity(&self) -> bool {
        !self.is_meta_query && self.plan_continuation_score > 0
    }

    /// Returns `true` when the prompt carries sufficient structural/action density to warrant AST/CodeGraph grounding.
    #[inline]
    #[must_use]
    pub const fn has_code_exploration_intent(&self) -> bool {
        !self.is_meta_query
            && self
                .action_score
                .saturating_add(self.code_morphology_score)
                .saturating_add(self.domain_score)
                >= crate::constants::CODE_EXPLORATION_MIN_ENERGY
    }
}

/// High-speed, declarative intent classifier for dynamic tool gating and signal vector projection.
pub struct IntentClassifier;

#[inline]
fn tokenize_words(lower: &str) -> HashSet<&str> {
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect()
}

#[inline]
fn has_file_extension_shape(token: &str) -> bool {
    let clean = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
    clean.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=crate::constants::FILE_EXT_MAX_LEN).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

#[inline]
fn count_code_morphology_tokens(prompt: &str) -> usize {
    prompt
        .split_whitespace()
        .filter(|raw| {
            let has_structural_punct = crate::constants::CODE_MORPHOLOGY_CHAR_MARKERS
                .iter()
                .any(|&ch| raw.contains(ch))
                || crate::constants::CODE_MORPHOLOGY_STR_MARKERS
                    .iter()
                    .any(|&pat| raw.contains(pat));
            let has_camel_transition = raw
                .as_bytes()
                .windows(2)
                .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
            has_structural_punct || has_camel_transition || has_file_extension_shape(raw)
        })
        .count()
}

impl IntentClassifier {
    /// Returns `true` when the user prompt is a conversational meta-introspection query about
    /// the agent's own tools, features, or skills (e.g., "how many tools were active now",
    /// "what are the features u have", "so how many tools u have in total"), which must NOT
    /// trigger domain tool category mutations.
    #[must_use]
    pub fn is_meta_introspection_query(prompt: &str) -> bool {
        let lower = prompt.to_ascii_lowercase();
        let words = tokenize_words(&lower);
        let has_subject = crate::constants::META_INTROSPECTION_SUBJECT_TOKENS
            .iter()
            .any(|s| words.contains(s));
        let has_cue = crate::constants::META_INTROSPECTION_QUERY_CUES
            .iter()
            .any(|c| words.contains(c));
        let has_action_verb = crate::constants::META_TASK_ACTION_VERBS
            .iter()
            .any(|v| words.contains(v));
        let has_agent_state_predication = crate::constants::META_AGENT_PRONOUN_TOKENS
            .iter()
            .any(|p| words.contains(p))
            && crate::constants::META_STATE_PREDICATE_TOKENS
                .iter()
                .any(|s| words.contains(s));
        let has_cardinality_interrogative = words.contains("how") && words.contains("many");

        has_subject
            && has_cue
            && (!has_action_verb || has_agent_state_predication || has_cardinality_interrogative)
    }

    fn score_rules(lower: &str, words: &HashSet<&str>) -> Vec<(usize, &'static [ToolCategory])> {
        let trimmed = lower.trim();
        let opening_end = lower.floor_char_boundary(
            lower
                .len()
                .min(crate::constants::INTENT_OPENING_CLAUSE_CHAR_WINDOW),
        );
        let opening = &lower[..opening_end];
        let opening_words = tokenize_words(opening);
        let w_open = crate::constants::INTENT_OPENING_CLAUSE_WEIGHT;

        let mut scored_rules: Vec<(usize, &'static [ToolCategory])> = INTENT_RULES
            .iter()
            .filter_map(|rule| {
                let word_score: usize = rule
                    .exact_words
                    .iter()
                    .map(|w| {
                        usize::from(opening_words.contains(w)) * w_open
                            + usize::from(!opening_words.contains(w) && words.contains(w))
                    })
                    .sum();
                let phrase_score: usize = rule
                    .phrases
                    .iter()
                    .map(|p| {
                        usize::from(opening.contains(p)) * (w_open * 2)
                            + usize::from(!opening.contains(p) && lower.contains(p)) * 2
                    })
                    .sum();
                let exact_score = usize::from(rule.exact_prompts.contains(&trimmed)) * (w_open * 3);
                let co_occ_score: usize = rule
                    .co_occurrences
                    .iter()
                    .filter(|(lhs, rhs)| {
                        lhs.iter().any(|l| words.contains(l))
                            && rhs.iter().any(|r| words.contains(r))
                    })
                    .count()
                    * (w_open * 2);

                let total = word_score + phrase_score + exact_score + co_occ_score;
                (total > 0).then_some((total, rule.categories))
            })
            .collect();

        scored_rules.sort_by(|a, b| b.0.cmp(&a.0));
        scored_rules
    }

    /// Projects a user prompt (and optional active plan/todo text) into a continuous
    /// `PromptSignalVector` in O(N) time without hardcoded greeting lists or regexes.
    #[must_use]
    pub fn analyze_signal(prompt: &str, active_plan_context: Option<&str>) -> PromptSignalVector {
        let is_meta_query = Self::is_meta_introspection_query(prompt);
        let lower = prompt.to_ascii_lowercase();
        let words = tokenize_words(&lower);
        let word_count = words.len();
        let active_mask = usize::from(!is_meta_query);

        let scored_rules = Self::score_rules(&lower, &words);
        let domain_score: usize = scored_rules.iter().map(|(s, _)| *s).sum::<usize>() * active_mask;

        let action_score = crate::constants::META_TASK_ACTION_VERBS
            .iter()
            .filter(|&&v| words.contains(v))
            .count()
            * active_mask;

        let code_morphology_score = count_code_morphology_tokens(prompt) * active_mask;

        let cue_hits = crate::constants::PLAN_CONTINUATION_CUE_TOKENS
            .iter()
            .filter(|&&c| words.contains(c))
            .count();

        let plan_overlap = active_plan_context
            .map(|plan_text| {
                let plan_lower = plan_text.to_ascii_lowercase();
                let plan_words: HashSet<&str> = tokenize_words(&plan_lower)
                    .into_iter()
                    .filter(|w| w.len() >= crate::constants::PLAN_AFFINITY_MIN_TOKEN_LEN)
                    .collect();
                words
                    .iter()
                    .filter(|&&w| {
                        w.len() >= crate::constants::PLAN_AFFINITY_MIN_TOKEN_LEN
                            && plan_words.contains(w)
                    })
                    .count()
            })
            .unwrap_or(0);

        let has_intent_carrier = usize::from(action_score + domain_score + cue_hits > 0);
        let plan_continuation_score = (cue_hits + plan_overlap * has_intent_carrier) * active_mask;

        PromptSignalVector {
            word_count,
            domain_score,
            action_score,
            code_morphology_score,
            plan_continuation_score,
            is_meta_query,
        }
    }

    /// Detects relevant tool categories from user prompt text using opening-clause-weighted
    /// intent scoring and a `MAX_INTENT_AUTO_CATEGORIES` cap so multi-paragraph prompts do not
    /// flood the active toolset with every mentioned domain.
    pub fn detect(prompt: &str) -> HashSet<ToolCategory> {
        let mut categories = HashSet::new();
        if Self::is_meta_introspection_query(prompt) {
            return categories;
        }

        let lower = prompt.to_ascii_lowercase();
        let words = tokenize_words(&lower);
        let scored_rules = Self::score_rules(&lower, &words);

        for (_, rule_cats) in scored_rules {
            if categories.len() >= crate::constants::MAX_INTENT_AUTO_CATEGORIES {
                break;
            }
            for &cat in rule_cats {
                if categories.len() < crate::constants::MAX_INTENT_AUTO_CATEGORIES
                    || categories.contains(&cat)
                {
                    categories.insert(cat);
                }
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

    #[test]
    fn test_meta_introspection_queries_do_not_trigger_domain_categories() {
        for query in [
            "what are the features u have",
            "what are the tools u have",
            "so how many tools were active now",
            "so how many tools u have in total",
            "so for this project u make what are the skills,tools,features u used",
        ] {
            assert!(
                IntentClassifier::is_meta_introspection_query(query),
                "Expected '{}' to be classified as meta introspection",
                query
            );
            assert!(
                IntentClassifier::detect(query).is_empty(),
                "Expected '{}' to return empty tool categories",
                query
            );
        }
    }

    #[test]
    fn test_intent_detection_capped_at_max_auto_categories() {
        let noisy_prompt = "build a landing page with navbar hero pricing table git commit push search web url fetch minitask dev server";
        let detected = IntentClassifier::detect(noisy_prompt);
        assert!(
            detected.len() <= crate::constants::MAX_INTENT_AUTO_CATEGORIES,
            "Expected at most {} categories, got {}: {:?}",
            crate::constants::MAX_INTENT_AUTO_CATEGORIES,
            detected.len(),
            detected
        );
    }

    #[test]
    fn test_prompt_signal_vector_zero_energy_vs_actionable_and_plan_affinity() {
        let active_plan = Some("Implement OAuth2 JWT token validation in src/auth/jwt.rs");

        for conversational in ["hii", "good morning", "hey there", "thanks a lot", "yo"] {
            let sig = IntentClassifier::analyze_signal(conversational, active_plan);
            assert_eq!(
                sig.task_energy(),
                0,
                "Expected zero task energy for '{}'",
                conversational
            );
            assert!(!sig.has_actionable_task_intent());
            assert!(!sig.has_plan_continuation_affinity());
            assert!(!sig.has_code_exploration_intent());
        }

        // Unrelated engineering question should have actionable intent but ZERO plan continuation affinity
        let unrelated = IntentClassifier::analyze_signal(
            "explain how the rust borrow checker works",
            active_plan,
        );
        assert!(unrelated.has_actionable_task_intent());
        assert!(!unrelated.has_plan_continuation_affinity());

        // Explicit continuation or plan-overlapping task prompt should have plan affinity
        let cont = IntentClassifier::analyze_signal("continue the todo plan", active_plan);
        assert!(cont.has_actionable_task_intent());
        assert!(cont.has_plan_continuation_affinity());

        let overlap =
            IntentClassifier::analyze_signal("implement the oauth2 token validation", active_plan);
        assert!(overlap.has_actionable_task_intent());
        assert!(overlap.has_plan_continuation_affinity());

        // Code morphology detection
        let code_sig =
            IntentClassifier::analyze_signal("fix bug in src/auth/jwt.rs validateToken", None);
        assert!(code_sig.code_morphology_score >= 2);
        assert!(code_sig.has_code_exploration_intent());
    }
}
