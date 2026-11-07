//! Markdown AST (`pulldown-cmark`), Least-Privilege Set Algebra ($U \setminus D$),
//! Trigger Specificity Analyzer, Sliding-Window Token Co-Occurrence Intent Vector Scorer,
//! Passive SVG Inspector, and Structured JSON/Manifest Analyzer (`MCP` Tool Poisoning & Supply Chain).

use super::ast_taint::analyze_script_ast_and_taint;
use super::{ExercisedCapability, FindingCategory, FindingSeverity, SkillFinding};
use crate::constants::{
    SECRET_PATTERNS, SKILLSPECTOR_CLOUD_METADATA_HOSTS, SKILLSPECTOR_COMPACT_SNIPPET_LEN,
    SKILLSPECTOR_EXECUTABLE_HTML_PREFIXES, SKILLSPECTOR_INTENT_WINDOW_TOKENS, SSRF_BLOCKED_HOSTS,
};
use crate::vault::models::{SkillFrontmatter, SkillScope, VaultSkill};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;

struct IntentVectorRule {
    id: &'static str,
    category: FindingCategory,
    severity: FindingSeverity,
    title: &'static str,
    description: &'static str,
    verbs: &'static [&'static str],
    targets: &'static [&'static str],
    contexts: &'static [&'static str],
}

const INTENT_VECTOR_RULES: &[IntentVectorRule] = &[
    IntentVectorRule {
        id: "P1",
        category: FindingCategory::PromptInjection,
        severity: FindingSeverity::Critical,
        title: "Instruction Override Directive",
        description: "Token window exhibits high co-occurrence of imperative override verbs targeting prior/system instructions.",
        verbs: &["ignor", "disregard", "forget", "overrid", "discard", "abandon"],
        targets: &["previous", "prior", "abov", "system", "earli", "origin", "preced"],
        contexts: &["instruct", "prompt", "rule", "direct", "guidanc", "constraint"],
    },
    IntentVectorRule {
        id: "AR1",
        category: FindingCategory::AntiRefusal,
        severity: FindingSeverity::Critical,
        title: "Anti-Refusal / Guardrail Bypass Directive",
        description: "Token window attempts to disable or bypass safety policies, refusal checks, or security guardrails.",
        verbs: &["bypass", "disabl", "deactiv", "suppres", "suspend", "turn"],
        targets: &["safeti", "guardrail", "refus", "polici", "filter", "ethic", "align"],
        contexts: &["check", "mode", "restrict", "constraint", "polici", "guardrail", "off"],
    },
    IntentVectorRule {
        id: "P6",
        category: FindingCategory::PromptInjection,
        severity: FindingSeverity::High,
        title: "System Prompt Leakage / Extraction Directive",
        description: "Token window instructs the agent to dump, repeat, or reveal its hidden system prompt or developer instructions.",
        verbs: &["reveal", "print", "dump", "output", "repeat", "expos", "disclos"],
        targets: &["system", "hidden", "develop", "initi", "confidential"],
        contexts: &["prompt", "instruct", "verbatim", "exact", "messag"],
    },
    IntentVectorRule {
        id: "MP1",
        category: FindingCategory::MemoryPoisoning,
        severity: FindingSeverity::High,
        title: "Silent Persistent Agent Config / Memory Poisoning",
        description: "Token window instructs silent modification of persistent agent configuration or memory files.",
        verbs: &["append", "inject", "overwrit", "plant", "insert", "tamper"],
        targets: &[
            "claude.md",
            "agents.md",
            "soul.md",
            ".cursorrules",
            "memory.json",
            "mcp.json",
            "minicode.md",
        ],
        contexts: &["silent", "hidden", "without", "secret", "background", "backdoor"],
    },
    IntentVectorRule {
        id: "EA1",
        category: FindingCategory::ExcessiveAgency,
        severity: FindingSeverity::High,
        title: "Unattended Confirmation / Approval Bypass",
        description: "Token window directs the agent to bypass user approval gates or execute destructive actions silently.",
        verbs: &["skip", "bypass", "omit", "never", "dont", "avoid"],
        targets: &["confirm", "approv", "consent", "permiss", "verif"],
        contexts: &["user", "human", "ask", "without", "silent"],
    },
];

const DEFENSIVE_NEGATION_STEMS: &[&str] = &[
    "never", "not", "dont", "avoid", "prevent", "block", "reject", "detect", "audit", "forbid",
    "prohibit", "flag", "malici", "attack", "vulner",
];

const GENERIC_CATCH_ALL_TRIGGERS: &[&str] = &[
    "*", "all", "any", "the", "and", "for", "with", "code", "file", "app", "run", "help", "fix",
    "do", "make", "test", "use",
];

const HTML_COMMENT_DIRECTIVE_CUES: &[&str] =
    &["instruction", "system", "always", "execute", "curl", "http"];

const SVG_ACTIVE_EVENT_MARKERS: &[&str] = &[
    "<script",
    "<foreignobject",
    "onload=",
    "onerror=",
    "onclick=",
    "onmouseover=",
    "javascript:",
];

const PACKAGE_LIFECYCLE_HOOKS: &[&str] = &["preinstall", "install", "postinstall", "prepare"];

const REDIRECT_INDEX_FLAGS: &[&str] = &[
    "--index-url",
    "--extra-index-url",
    "--trusted-host",
    "--find-links",
];

/// Declarative mapping from tool names in `allowed_tools` to `ExercisedCapability` domains.
const TOOL_CAPABILITY_TABLE: &[(&[&str], ExercisedCapability)] = &[
    (&["*", "all"], ExercisedCapability::Wildcard),
    (
        &["exec_cmd", "sandbox_exec", "bash", "sh", "minitask"],
        ExercisedCapability::Exec,
    ),
    (
        &[
            "write_file",
            "patch_file",
            "replace_in_files",
            "delete_file",
            "ast_replace_node",
            "ast_grep_rewrite",
            "edit",
            "write",
        ],
        ExercisedCapability::Write,
    ),
    (
        &[
            "webfetch",
            "fetch",
            "search_web",
            "browser_navigate",
            "vault_import_url",
            "vault_ingest_source",
        ],
        ExercisedCapability::Network,
    ),
    (
        &[
            "read_file",
            "batch_read",
            "list_dir",
            "grep_search",
            "file_search",
            "read",
        ],
        ExercisedCapability::Read,
    ),
];

/// Binary-searches 0-based byte offsets of line starts to return a 1-based line number in $O(\log N)$ time.
fn byte_offset_to_line(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|&start| start <= offset).max(1)
}

fn compute_line_starts(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (idx, b) in content.bytes().enumerate() {
        if b == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

/// Parses `SKILL.md` using `pulldown-cmark` AST events, evaluates embedded HTML/links/images/code fences,
/// and runs sliding-window intent vector scoring on prose and comments.
pub fn analyze_markdown_structure_and_intent(
    rel_path: &str,
    content: &str,
) -> (Vec<SkillFinding>, HashSet<ExercisedCapability>) {
    let mut findings = Vec::new();
    let mut capabilities = HashSet::new();
    let line_starts = compute_line_starts(content);

    let parser = Parser::new_ext(content, Options::all()).into_offset_iter();

    let mut active_fence_lang: Option<(String, usize)> = None;
    let mut active_fence_buf = String::new();
    let mut active_image_url: Option<(String, usize)> = None;

    for (event, range) in parser {
        let line_num = byte_offset_to_line(&line_starts, range.start);

        match event {
            Event::Html(html_str) | Event::InlineHtml(html_str) => {
                analyze_html_node(rel_path, line_num, &html_str, &mut findings);
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                analyze_url_target(rel_path, line_num, &dest_url, false, &mut findings);
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                analyze_url_target(rel_path, line_num, &dest_url, true, &mut findings);
                active_image_url = Some((dest_url.to_string(), line_num));
            }
            Event::End(TagEnd::Image) => {
                active_image_url = None;
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                let tag = lang
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                active_fence_lang = Some((tag, line_num));
                active_fence_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((lang_tag, fence_line)) = active_fence_lang.take() {
                    let effective_tag = if lang_tag.is_empty() {
                        "sh"
                    } else {
                        lang_tag.as_str()
                    };
                    if effective_tag.eq_ignore_ascii_case("json") {
                        findings.extend(analyze_mcp_and_package_json(rel_path, &active_fence_buf));
                    } else {
                        let (ast_findings, ast_caps) = analyze_script_ast_and_taint(
                            rel_path,
                            effective_tag,
                            &active_fence_buf,
                            fence_line,
                        );
                        findings.extend(ast_findings);
                        capabilities.extend(ast_caps);
                    }
                    active_fence_buf.clear();
                }
            }
            Event::Text(text_str) => {
                if active_fence_lang.is_some() {
                    active_fence_buf.push_str(&text_str);
                } else if active_image_url.is_none() {
                    analyze_prose_intent_vectors(
                        rel_path,
                        line_num,
                        &text_str,
                        false,
                        &mut findings,
                    );
                }
            }
            _ => {}
        }
    }

    (findings, capabilities)
}

fn analyze_html_node(
    rel_path: &str,
    line_num: usize,
    html: &str,
    findings: &mut Vec<SkillFinding>,
) {
    let trimmed = html.trim();
    let lower = trimmed.to_ascii_lowercase();

    if SKILLSPECTOR_EXECUTABLE_HTML_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        findings.push(SkillFinding::new(
            "P2",
            FindingCategory::PromptInjection,
            FindingSeverity::High,
            0.93,
            rel_path,
            line_num,
            "Embedded Executable HTML Tag in Markdown",
            "Skill markdown embeds active HTML tags (<script>, <iframe>, <object>, <embed>) that can execute code or load external frames.",
            trimmed
                .chars()
                .take(SKILLSPECTOR_COMPACT_SNIPPET_LEN)
                .collect::<String>(),
            "Remove raw <script>, <iframe>, <object>, and <embed> tags from skill markdown.",
        ));
        return;
    }

    if let Some(inner) = trimmed
        .strip_prefix("<!--")
        .and_then(|s| s.strip_suffix("-->"))
    {
        let inner_trimmed = inner.trim();
        if inner_trimmed.is_empty() || inner_trimmed.eq_ignore_ascii_case("slide") {
            return;
        }

        let before_len = findings.len();
        analyze_prose_intent_vectors(rel_path, line_num, inner_trimmed, true, findings);

        if findings.len() == before_len && inner_trimmed.split_whitespace().count() >= 5 {
            let lower_inner = inner_trimmed.to_ascii_lowercase();
            let has_directive_cue = HTML_COMMENT_DIRECTIVE_CUES
                .iter()
                .any(|cue| lower_inner.contains(cue));
            if has_directive_cue {
                findings.push(SkillFinding::new(
                    "P2",
                    FindingCategory::PromptInjection,
                    FindingSeverity::High,
                    0.88,
                    rel_path,
                    line_num,
                    "Hidden Directive Inside HTML Comment",
                    "Markdown HTML comment conceals imperative instructions invisible in rendered documentation views.",
                    inner_trimmed
                        .chars()
                        .take(SKILLSPECTOR_COMPACT_SNIPPET_LEN)
                        .collect::<String>(),
                    "Remove hidden instructions from HTML comments; keep all agent directives visible.",
                ));
            }
        }
    }
}

fn analyze_url_target(
    rel_path: &str,
    line_num: usize,
    raw_url: &str,
    is_image: bool,
    findings: &mut Vec<SkillFinding>,
) {
    let trimmed = raw_url.trim();
    let upper = trimmed.to_ascii_uppercase();
    let has_interpolated_var = trimmed.contains('{')
        || trimmed.contains('}')
        || trimmed.contains('$')
        || SECRET_PATTERNS.iter().any(|pat| upper.contains(pat));

    if is_image && has_interpolated_var {
        findings.push(SkillFinding::new(
            "E2",
            FindingCategory::DataExfiltration,
            FindingSeverity::Critical,
            0.96,
            rel_path,
            line_num,
            "Markdown Image Beacon Data Exfiltration",
            "Markdown image URL interpolates dynamic variables or secrets into query parameters to exfiltrate context upon rendering.",
            trimmed
                .chars()
                .take(SKILLSPECTOR_COMPACT_SNIPPET_LEN)
                .collect::<String>(),
            "Remove dynamic variable interpolation from external Markdown image URLs.",
        ));
    }

    let Ok(parsed) = url::Url::parse(trimmed) else {
        return;
    };
    let Some(host) = parsed.host_str() else {
        return;
    };
    let host_lower = host.to_ascii_lowercase();

    if SKILLSPECTOR_CLOUD_METADATA_HOSTS.contains(&host_lower.as_str()) {
        findings.push(SkillFinding::new(
            "SSRF1",
            FindingCategory::Ssrf,
            FindingSeverity::Critical,
            0.98,
            rel_path,
            line_num,
            "Cloud Instance Metadata SSRF Endpoint",
            "URL targets a cloud instance metadata service (169.254.169.254 / metadata.google.internal) to harvest IAM credentials.",
            trimmed.to_string(),
            "Never access cloud metadata endpoints from skill instructions or scripts.",
        ));
        return;
    }

    if is_image && SSRF_BLOCKED_HOSTS.contains(&host_lower.as_str()) {
        findings.push(SkillFinding::new(
            "SSRF2",
            FindingCategory::Ssrf,
            FindingSeverity::Medium,
            0.85,
            rel_path,
            line_num,
            "Internal Loopback Image Beacon",
            "Markdown image references a localhost/loopback endpoint which can trigger unintended local service requests.",
            trimmed.to_string(),
            "Avoid embedding loopback URLs inside auto-loaded Markdown image tags.",
        ));
    }
}

/// Audits an `.svg` vector image (`passive_svg.py`) for embedded active `<script>`, `<foreignObject>`, or event handlers.
pub fn analyze_svg_document(rel_path: &str, content: &str) -> Vec<SkillFinding> {
    let mut findings = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        if let Some(&marker) = SVG_ACTIVE_EVENT_MARKERS
            .iter()
            .find(|&&m| lower.contains(m))
        {
            findings.push(SkillFinding::new(
                "P2",
                FindingCategory::PromptInjection,
                FindingSeverity::High,
                0.94,
                rel_path,
                idx + 1,
                "Active Executable Payload Inside SVG Image",
                "SVG image contains active script tags, foreignObject frames, or inline JavaScript event handlers instead of passive vector paths.",
                format!("marker='{}', line={}", marker, line.trim()),
                "Strip all <script>, <foreignObject>, and on* event attributes from SVG documentation assets.",
            ));
        }
    }
    findings
}

/// Audits JSON configurations (`mcp.json`, `.mcp.json`, `package.json`) using `serde_json::Value` AST traversal
/// for MCP Tool Poisoning (`TP3`), MCP Unpinned Rug-Pull (`TP4`), and Supply-Chain Lifecycle Hooks (`SC5`).
pub fn analyze_mcp_and_package_json(rel_path: &str, content: &str) -> Vec<SkillFinding> {
    let mut findings = Vec::new();
    let Ok(root) = serde_json::from_str::<serde_json::Value>(content) else {
        return findings;
    };

    // 1. Inspect `mcpServers` or `servers` object (`mcp_tool_poisoning.py` / `mcp_rug_pull.py`)
    let servers_obj = root
        .get("mcpServers")
        .or_else(|| root.get("servers"))
        .and_then(|v| v.as_object());

    if let Some(servers) = servers_obj {
        for (srv_name, srv_cfg) in servers {
            let args_unpinned = srv_cfg
                .get("args")
                .and_then(|v| v.as_array())
                .is_some_and(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .any(|arg| arg.ends_with("@latest") || arg.starts_with("http://"))
                });
            let url_unpinned = srv_cfg
                .get("url")
                .and_then(|v| v.as_str())
                .is_some_and(|u| u.starts_with("http://"));

            if args_unpinned || url_unpinned {
                findings.push(SkillFinding::new(
                    "TP4",
                    FindingCategory::ToolPoisoning,
                    FindingSeverity::High,
                    0.89,
                    rel_path,
                    1,
                    "Unpinned or Cleartext MCP Server Definition (Rug-Pull Risk)",
                    "MCP server configuration uses unpinned `@latest` package execution or unencrypted `http://` transport, enabling post-install rug-pull mutation.",
                    format!("mcpServer='{}'", srv_name),
                    "Pin MCP server packages to an immutable version/digest and use TLS (`https://`) endpoints.",
                ));
            }
        }
    }

    // 2. Recursively inspect `"description"` / `"instructions"` string fields in JSON for hidden prompt injection (`TP3`)
    inspect_json_descriptions_recursive(rel_path, &root, &mut findings);

    // 3. Inspect `package.json` `"scripts"` lifecycle hooks (`SC5`)
    if let Some(scripts) = root.get("scripts").and_then(|v| v.as_object()) {
        for &hook in PACKAGE_LIFECYCLE_HOOKS {
            if let Some(cmd) = scripts.get(hook).and_then(|v| v.as_str()) {
                let lower = cmd.to_ascii_lowercase();
                if [
                    "curl",
                    "wget",
                    "bash",
                    "sh ",
                    "node -e",
                    "python -c",
                    "eval",
                ]
                .iter()
                .any(|m| lower.contains(m))
                {
                    findings.push(SkillFinding::new(
                        "SC5",
                        FindingCategory::SupplyChain,
                        FindingSeverity::Critical,
                        0.95,
                        rel_path,
                        1,
                        "Suspicious Package Lifecycle Install Hook",
                        "package.json defines an install lifecycle hook (preinstall/postinstall) that executes shell or network commands automatically during dependency installation.",
                        format!("scripts.{} = '{}'", hook, cmd),
                        "Remove network/shell execution from package.json install lifecycle hooks.",
                    ));
                }
            }
        }
    }

    findings
}

fn inspect_json_descriptions_recursive(
    rel_path: &str,
    val: &serde_json::Value,
    findings: &mut Vec<SkillFinding>,
) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "description" | "instructions" | "systemPrompt" | "prompt"
                ) {
                    if let Some(text) = v.as_str() {
                        let before = findings.len();
                        analyze_prose_intent_vectors(rel_path, 1, text, true, findings);
                        for f in &mut findings[before..] {
                            f.id = "TP3".to_string();
                            f.category = FindingCategory::ToolPoisoning;
                            f.title = "MCP / Schema Tool Description Poisoning".to_string();
                        }
                    }
                }
                inspect_json_descriptions_recursive(rel_path, v, findings);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                inspect_json_descriptions_recursive(rel_path, item, findings);
            }
        }
        _ => {}
    }
}

/// Audits `requirements.txt` or `pyproject.toml` for untrusted package index redirection (`SC1`).
pub fn analyze_dependency_manifest(rel_path: &str, content: &str) -> Vec<SkillFinding> {
    let mut findings = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if REDIRECT_INDEX_FLAGS.iter().any(|flag| line.contains(flag)) {
            findings.push(SkillFinding::new(
                "SC1",
                FindingCategory::SupplyChain,
                FindingSeverity::High,
                0.93,
                rel_path,
                idx + 1,
                "Untrusted Package Registry Redirection in Dependency Manifest",
                "Dependency manifest overrides the package index URL or disables host TLS verification.",
                line.to_string(),
                "Remove custom `--index-url`, `--extra-index-url`, and `--trusted-host` directives from skill dependency files.",
            ));
        }
    }
    findings
}

fn analyze_prose_intent_vectors(
    rel_path: &str,
    line_num: usize,
    text: &str,
    inside_hidden_comment: bool,
    findings: &mut Vec<SkillFinding>,
) {
    let raw_tokens: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-')
        .filter(|t| t.len() >= 2)
        .collect();
    if raw_tokens.len() < 3 {
        return;
    }

    let stems: Vec<String> = raw_tokens
        .iter()
        .map(|t| {
            let lower = t.to_ascii_lowercase();
            VaultSkill::normalize_stem(&lower).to_string()
        })
        .collect();

    let win_size = SKILLSPECTOR_INTENT_WINDOW_TOKENS.min(stems.len());
    for rule in INTENT_VECTOR_RULES {
        for window in stems.windows(win_size) {
            let has_verb = window.iter().any(|s| {
                rule.verbs
                    .iter()
                    .any(|&v| s == v || (v.len() >= 4 && s.starts_with(v)))
            });
            let has_target = window.iter().any(|s| {
                rule.targets
                    .iter()
                    .any(|&t| s == t || (t.len() >= 4 && s.starts_with(t)))
            });
            let has_context = window.iter().any(|s| {
                rule.contexts
                    .iter()
                    .any(|&c| s == c || (c.len() >= 4 && s.starts_with(c)))
            });

            if !(has_verb && has_target && has_context) {
                continue;
            }

            let is_defensive_rule = rule.id != "EA1"
                && !inside_hidden_comment
                && window.iter().any(|s| {
                    DEFENSIVE_NEGATION_STEMS
                        .iter()
                        .any(|&neg| s == neg || (neg.len() >= 5 && s.starts_with(neg)))
                });
            if is_defensive_rule {
                continue;
            }

            let severity = if inside_hidden_comment {
                FindingSeverity::Critical
            } else {
                rule.severity
            };
            findings.push(SkillFinding::new(
                rule.id,
                rule.category,
                severity,
                if inside_hidden_comment { 0.96 } else { 0.88 },
                rel_path,
                line_num,
                rule.title,
                rule.description,
                text.trim()
                    .chars()
                    .take(SKILLSPECTOR_COMPACT_SNIPPET_LEN)
                    .collect::<String>(),
                "Remove adversarial instruction override, anti-refusal, or silent tampering directives.",
            ));
            break;
        }
    }
}

/// Maps a declared tool name or pattern in `allowed_tools` to its corresponding capability domain via table lookup.
pub fn map_tool_name_to_capability(tool_name: &str) -> Option<ExercisedCapability> {
    let normalized = tool_name
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    TOOL_CAPABILITY_TABLE
        .iter()
        .find(|(names, _)| names.contains(&normalized.as_str()))
        .map(|(_, cap)| *cap)
}

/// Evaluates Least-Privilege Set Difference ($\text{Exercised} \setminus \text{Declared}$) (`LP1`–`LP3`)
/// and Trigger Specificity / Shadowing (`TR1`, `TR3`).
pub fn analyze_privilege_and_triggers(
    rel_path: &str,
    frontmatter: &SkillFrontmatter,
    scope: SkillScope,
    has_executable_scripts: bool,
    exercised_from_scripts: &HashSet<ExercisedCapability>,
    builtin_skill_names: &HashSet<String>,
) -> Vec<SkillFinding> {
    let mut findings = Vec::new();

    // 1. Least-Privilege Set Difference (LP1, LP2, LP3)
    let declared_caps: HashSet<ExercisedCapability> = frontmatter
        .allowed_tools
        .iter()
        .filter_map(|t| map_tool_name_to_capability(t))
        .collect();

    if declared_caps.contains(&ExercisedCapability::Wildcard) {
        findings.push(SkillFinding::new(
            "LP2",
            FindingCategory::LeastPrivilege,
            FindingSeverity::Medium,
            0.92,
            rel_path,
            1,
            "Wildcard Tool Permission Grant",
            "Skill frontmatter declares wildcard (`*` or `all`) in `allowed_tools`, violating the principle of least privilege.",
            format!("allowed_tools={:?}", frontmatter.allowed_tools),
            "Restrict `allowed_tools` to the explicit minimum set of tools required by the skill.",
        ));
    } else if !frontmatter.allowed_tools.is_empty() {
        for &cap in exercised_from_scripts {
            if matches!(
                cap,
                ExercisedCapability::Exec
                    | ExercisedCapability::Network
                    | ExercisedCapability::Write
            ) && !declared_caps.contains(&cap)
            {
                findings.push(SkillFinding::new(
                    "LP1",
                    FindingCategory::LeastPrivilege,
                    FindingSeverity::Medium,
                    0.87,
                    rel_path,
                    1,
                    "Undeclared Capability Exercised by Skill Scripts",
                    &format!(
                        "Skill scripts exercise `{}` capability which is absent from declared `allowed_tools`.",
                        cap.as_str()
                    ),
                    format!(
                        "exercised={}, declared={:?}",
                        cap.as_str(),
                        frontmatter.allowed_tools
                    ),
                    "Declare all exercised high-impact capabilities in `allowed_tools` or remove the capability from scripts.",
                ));
            }
        }
    } else if has_executable_scripts
        && (exercised_from_scripts.contains(&ExercisedCapability::Exec)
            || exercised_from_scripts.contains(&ExercisedCapability::Network))
    {
        findings.push(SkillFinding::new(
            "LP3",
            FindingCategory::LeastPrivilege,
            FindingSeverity::Low,
            0.80,
            rel_path,
            1,
            "Missing `allowed_tools` Manifest for Executable Skill",
            "Skill bundles executable scripts exercising command/network capabilities without declaring `allowed_tools` in frontmatter.",
            "allowed_tools=[]".to_string(),
            "Add an explicit `allowed_tools` list in YAML frontmatter documenting required capabilities.",
        ));
    }

    // 2. Trigger Specificity & Built-in Shadowing (TR1, TR3)
    let broad_triggers: Vec<&str> = frontmatter
        .triggers
        .iter()
        .map(|s| s.trim())
        .filter(|t| {
            t.len() <= 1
                || GENERIC_CATCH_ALL_TRIGGERS
                    .iter()
                    .any(|&g| t.eq_ignore_ascii_case(g))
        })
        .collect();

    if !broad_triggers.is_empty() {
        findings.push(SkillFinding::new(
            "TR1",
            FindingCategory::TriggerQuality,
            FindingSeverity::Medium,
            0.90,
            rel_path,
            1,
            "Overly Generic Catch-All Trigger Keyword",
            "Skill defines high-frequency stop-words or single-character wildcards as triggers, causing indiscriminate context activation.",
            format!("broad_triggers={:?}", broad_triggers),
            "Use specific, domain-scoped trigger keywords (>= 3 characters).",
        ));
    }

    if scope != SkillScope::Builtin {
        for trig in &frontmatter.triggers {
            let norm = trig.trim().to_ascii_lowercase();
            if norm != frontmatter.name.to_ascii_lowercase() && builtin_skill_names.contains(&norm)
            {
                findings.push(SkillFinding::new(
                    "TR3",
                    FindingCategory::TriggerQuality,
                    FindingSeverity::Low,
                    0.82,
                    rel_path,
                    1,
                    "Trigger Collides With Built-In Core Skill Identifier",
                    &format!(
                        "External skill trigger `{}` shadows the identifier of built-in core skill `{}`.",
                        norm, norm
                    ),
                    format!("trigger='{}'", norm),
                    "Use distinct trigger phrases to avoid unintended shadowing of built-in skills.",
                ));
            }
        }
    }

    findings
}
