use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "remember_fact".to_string(),
            description: "Save a persistent fact, convention, or developer preference to Core Memory (survives across sessions).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Unique identifier for this memory (e.g. 'code_style', 'architecture')"
                    },
                    "value": {
                        "type": "string",
                        "description": "The fact, convention, or preference to remember"
                    },
                    "is_global": {
                        "type": "boolean",
                        "description": "Whether to store globally across all projects (~/.config/minicode/memory.json) or locally (.minicode/memory.json). Default: false (local)"
                    },
                    "category": {
                        "type": "string",
                        "enum": ["preference", "project_fact", "pattern"],
                        "description": "Category of memory (default: 'project_fact')"
                    }
                },
                "required": ["key", "value"]
            }),
        },
        ToolSchema {
            name: "update_fact".to_string(),
            description: "Update an existing fact or preference in Core Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key of the memory to update"
                    },
                    "new_value": {
                        "type": "string",
                        "description": "The updated fact or preference text"
                    }
                },
                "required": ["key", "new_value"]
            }),
        },
        ToolSchema {
            name: "forget_fact".to_string(),
            description: "Remove a fact or preference from Core Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key of the memory to remove"
                    }
                },
                "required": ["key"]
            }),
        },
        ToolSchema {
            name: "create_plan".to_string(),
            description: "Initialize a right-sized Two-Tier Task Plan in Working Memory on ANY turn (2-3 steps for small/targeted tasks, bugfixes, or audits; 4-8+ steps for medium/large features or full apps). Call `update_progress(step=\"active\", status=\"completed\")` immediately as each step or sub-step completes rather than batching at the end; include nested `substeps` whenever a macro step bundles multiple distinct sections/components or multi-stage verification (static build/lint + live browser/viewport QA). Writes the technical blueprint to `minikit_docs/core/implementation.md`, the milestone roadmap to `minikit_docs/core/todo.md` (viewable via /todo modal), and active tactical execution steps to `.minicode/plan/task_plan.md` rendered live in the TUI.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Short, descriptive title of the task or milestone"
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "anyOf": [
                                { "type": "string" },
                                {
                                    "type": "object",
                                    "properties": {
                                        "title": { "type": "string" },
                                        "description": { "type": "string" },
                                        "substeps": {
                                            "type": "array",
                                            "items": { "type": "string" },
                                            "description": "Tier-2 tactical sub-steps nested under this macro task (recommended whenever a phase builds 2+ sections/components or runs multi-stage verification)."
                                        }
                                    }
                                }
                            ]
                        },
                        "description": "Ordered list of macro tasks or tactical action steps (optionally with nested `substeps` per task)"
                    },
                    "architecture": {
                        "type": "string",
                        "description": "Comprehensive top-to-bottom technical architecture and engineering blueprint. Saved into `minikit_docs/core/implementation.md`."
                    },
                    "milestones": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "High-level roadmap milestones. Saved into `minikit_docs/core/todo.md` (viewable via /todo modal)."
                    },
                    "assumptions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Choices made without asking because they are cheap to change (e.g. 'Store notes in a JSON file'). Recorded as unconfirmed for the user to review."
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Activate the plan even if plan review reports blocking structural problems (the override is recorded in `.minicode/decisions.md`). Default false."
                    }
                },
                "required": ["title", "steps"]
            }),
        },
        ToolSchema {
            name: "read_plan".to_string(),
            description: "Read the active task plan, progress tracker, and findings from Working Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "log_finding".to_string(),
            description: "Record an architectural discovery, symbol location, or observation into findings.md.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "finding": {
                        "type": "string",
                        "description": "The observation, discovery, or architectural note to record"
                    }
                },
                "required": ["finding"]
            }),
        },
        ToolSchema {
            name: "record_decision".to_string(),
            description: "Record or update in-place a numbered architectural decision (D1, D2, ...) in `.minicode/decisions.md`. Each entry states a decision as it holds now; passing an existing ID (e.g. 'D1') or matching title edits that decision in place rather than appending contradictory history.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id_or_title": {
                        "type": "string",
                        "description": "Decision title (e.g. 'Inline Key Prefix') or existing decision ID to update in place (e.g. 'D1' or 'D1: Inline Key Prefix')"
                    },
                    "decision": {
                        "type": "string",
                        "description": "The architectural invariant or design decision as it holds now"
                    },
                    "rationale": {
                        "type": "string",
                        "description": "Optional cost-model or engineering rationale for why this trade-off was chosen"
                    }
                },
                "required": ["id_or_title", "decision"]
            }),
        },
        ToolSchema {
            name: "update_progress".to_string(),
            description: "Immediately update the status of the active task step (`step=\"active\"`, `status=\"completed\"`) or a specific step right when its work finishes so the live TUI Todo tree advances step-by-step in real time (never defer `update_progress` calls to the end). Completing a step automatically promotes the next pending step to `[>]` `in_progress` — NEVER emit a second `update_progress(..., status=\"in_progress\")` call.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "step": {
                        "type": "string",
                        "description": "The description or 1-based number of the step matching the task plan"
                    },
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of steps to update in a single call"
                    },
                    "status": {
                        "type": "string",
                        "description": "The status (e.g. 'Completed', 'In Progress', 'Blocked')"
                    }
                },
                "required": ["status"]
            }),
        },
        ToolSchema {
            name: "archive_plan".to_string(),
            description: "Archive the completed task plan and clear the active Working Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "checkpoint_session".to_string(),
            description: "Capture an immutable point-in-time snapshot of the current session conversation and working memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "Short mnemonic label for the checkpoint (e.g. 'before-ast-refactor')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional detailed context or rationale for creating this checkpoint"
                    }
                },
                "required": ["label"]
            }),
        },
        ToolSchema {
            name: "rewind_session".to_string(),
            description: "List checkpoints, rewind working memory to an earlier snapshot, or fork a new exploration branch.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "rewind", "fork"],
                        "description": "Action to perform: 'list' all checkpoints, 'rewind' to checkpoint, or 'fork' a new branch"
                    },
                    "checkpoint_id": {
                        "type": "string",
                        "description": "Target checkpoint ID (required for 'rewind' and 'fork')"
                    },
                    "fork_label": {
                        "type": "string",
                        "description": "Optional label when forking a checkpoint"
                    }
                },
                "required": ["action"]
            }),
        },
    ]
}

fn is_generic_plan_title(raw: &str) -> bool {
    let norm = raw.trim().to_ascii_lowercase();
    norm.is_empty()
        || matches!(
            norm.as_str(),
            "task plan"
                | "plan"
                | "development plan"
                | "implementation plan"
                | "project plan"
                | "todo"
                | "tasks"
                | "milestone"
                | "active milestone"
        )
}

fn clamp_title_words(raw: &str, max_chars: usize) -> String {
    let clean = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= max_chars {
        return clean;
    }
    let mut out = String::new();
    for word in clean.split_whitespace() {
        let next_len = if out.is_empty() {
            word.chars().count()
        } else {
            out.chars().count() + 1 + word.chars().count()
        };
        if next_len > max_chars && !out.is_empty() {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Returns true when `candidate` structurally names only Phase 1 / Step 1 of a multi-phase plan
/// rather than the holistic project objective (e.g., when an XML-to-JSON gateway spilled Step 1's
/// `<title>` onto top-level `"title"` while `milestones` / `steps` has multiple phases).
fn is_phase_one_specific_title(candidate: &str, args: &serde_json::Value) -> bool {
    use crate::context::working_memory::WorkingMemory;

    let steps_arr = ["steps", "tasks", "plan"]
        .iter()
        .find_map(|k| args.get(*k)?.as_array());
    let milestones = param::opt_string_array(args, "milestones")
        .or_else(|| param::opt_string_array(args, "roadmap"));

    let multi_phase = steps_arr.is_some_and(|a| a.len() >= 2)
        || milestones.as_ref().is_some_and(|m| m.len() >= 2);
    if !multi_phase {
        return false;
    }

    // 1. Exact collision with Step 1's own `title` / `name` field on a multi-phase plan
    if let Some(first_step_title) = steps_arr
        .and_then(|a| a.first())
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("title").or_else(|| o.get("name")))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if first_step_title.eq_ignore_ascii_case(candidate.trim()) {
            return true;
        }
    }

    // 2. Domain-token concentration in Milestone 1 vs Milestones 2..N
    if let Some(ref ms) = milestones {
        if ms.len() >= 2 {
            let cand_tokens = WorkingMemory::extract_domain_tokens(candidate);
            if cand_tokens.len() >= 2 {
                let m0_lower = ms[0].to_ascii_lowercase();
                let in_m0 = cand_tokens
                    .iter()
                    .filter(|t| WorkingMemory::stem_token_matches_task(t, &m0_lower))
                    .count();
                let in_later = cand_tokens
                    .iter()
                    .filter(|t| {
                        ms[1..].iter().any(|later| {
                            WorkingMemory::stem_token_matches_task(t, &later.to_ascii_lowercase())
                        })
                    })
                    .count();
                if in_m0 * 3 >= cand_tokens.len() * 2 && in_later * 4 <= cand_tokens.len() {
                    return true;
                }
            }
        }
    }

    false
}

/// Resolves a descriptive plan/milestone title when the caller omits `title`, passes a generic
/// placeholder like `"Task Plan"`, or passes Step 1's phase title on a multi-phase plan,
/// falling back through `.minicode/decisions.md`, `.minicode/intent_anchor.json`, `architecture`,
/// and `milestones` before `"Task Plan"`.
pub(crate) fn resolve_plan_title(raw_args: &serde_json::Value, workspace_root: &Path) -> String {
    let normalized = crate::agent::providers::normalize_xml_gateway_wrappers(raw_args.clone());
    let args = &normalized;

    let explicit_opt =
        param::get_str_with_aliases(args, &["title", "name", "goal", "topic", "milestone"])
            .map(str::trim)
            .filter(|s| !is_generic_plan_title(s));

    if let Some(explicit) = explicit_opt.filter(|s| !is_phase_one_specific_title(s, args)) {
        return clamp_title_words(explicit, 72);
    }

    // Check Living Decision Ledger (.minicode/decisions.md), e.g. `## [D1] minicode landing page — foundation (...)`
    let decisions_path = workspace_root.join(".minicode").join("decisions.md");
    if let Ok(decisions_md) = std::fs::read_to_string(&decisions_path) {
        for line in decisions_md.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("## [D") {
                if let Some((_, after_bracket)) = rest.split_once(']') {
                    let mut heading = after_bracket.trim();
                    if heading.to_ascii_lowercase().starts_with("assumed for ") {
                        continue;
                    }
                    if let Some(paren_idx) = heading.rfind(" (") {
                        let tail = &heading[paren_idx + 2..];
                        if tail.starts_with("answered by ")
                            || tail.starts_with("auto-default")
                            || tail.starts_with("not user-confirmed")
                            || tail.contains("UTC)")
                        {
                            heading = heading[..paren_idx].trim();
                        }
                    }
                    if let Some((left, _)) = heading.split_once(" — ") {
                        let left_trim = left.trim();
                        if left_trim.len() >= 4 && !is_generic_plan_title(left_trim) {
                            heading = left_trim;
                        }
                    } else if let Some((left, right)) = heading.rsplit_once(" - ") {
                        let left_trim = left.trim();
                        let right_trim = right.trim();
                        if left_trim.len() >= 4
                            && !is_generic_plan_title(left_trim)
                            && !right_trim.contains(char::is_whitespace)
                        {
                            heading = left_trim;
                        }
                    }
                    if !is_generic_plan_title(heading)
                        && !is_phase_one_specific_title(heading, args)
                    {
                        return clamp_title_words(heading, 72);
                    }
                }
            }
        }
    }

    // Check persisted IntentLedger root_objective (or legacy goal fields) if present
    let intent_path = workspace_root.join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
    if let Ok(raw_intent) = std::fs::read_to_string(&intent_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw_intent) {
            if let Some(goal) = val
                .get("root_objective")
                .or_else(|| val.get("primary_goal"))
                .or_else(|| val.get("goal"))
                .or_else(|| val.get("summary"))
                .and_then(|g| g.as_str())
                .map(str::trim)
                .filter(|g| !is_generic_plan_title(g) && *g != "General Assistance")
            {
                let first_clause = goal
                    .lines()
                    .next()
                    .unwrap_or(goal)
                    .split(['.', ';', '\n'])
                    .next()
                    .unwrap_or(goal)
                    .trim();
                if !is_generic_plan_title(first_clause) {
                    return clamp_title_words(first_clause, 64);
                }
            }
        }
    }

    // Check architecture / blueprint opening clause
    if let Some(arch) = param::opt_str(args, "architecture")
        .or_else(|| param::opt_str(args, "blueprint"))
        .or_else(|| param::opt_str(args, "implementation"))
    {
        let first_line = arch
            .lines()
            .map(|l| l.trim().trim_start_matches('#').trim())
            .find(|l| !l.is_empty())
            .unwrap_or("");
        let first_clause = first_line
            .split(['.', ';'])
            .next()
            .unwrap_or(first_line)
            .split(" — ")
            .next()
            .unwrap_or(first_line)
            .trim();
        if first_clause.len() >= 4 && !is_generic_plan_title(first_clause) {
            return clamp_title_words(first_clause, 64);
        }
    }

    if let Some(first_ms) = param::opt_string_array(args, "milestones")
        .or_else(|| param::opt_string_array(args, "roadmap"))
        .and_then(|arr| {
            arr.into_iter()
                .map(|s| s.trim().to_string())
                .find(|s| !is_generic_plan_title(s))
        })
    {
        return clamp_title_words(&first_ms, 72);
    }

    if let Some(explicit_fallback) = explicit_opt {
        return clamp_title_words(explicit_fallback, 72);
    }

    "Task Plan".to_string()
}

/// Promotes any project `milestones` not yet covered by `steps` (by morphological domain-token
/// coverage) into macro `steps`, preventing truncated 1-step plans when an upstream gateway or
/// model puts detailed substeps on Phase 1 while listing Phases 2..M in `milestones`.
fn reconcile_steps_with_milestones(
    steps: &mut Vec<String>,
    step_substeps: &mut Vec<Vec<String>>,
    milestones: Option<&[String]>,
) {
    use crate::context::working_memory::WorkingMemory;

    let Some(ms) = milestones else {
        return;
    };
    if ms.len() < 2 || steps.len() >= ms.len() || steps.is_empty() {
        return;
    }

    let existing_count = steps.len();
    for milestone in ms {
        let m_trimmed = milestone.trim();
        if m_trimmed.is_empty() {
            continue;
        }
        let m_tokens = WorkingMemory::extract_domain_tokens(m_trimmed);
        if m_tokens.is_empty() {
            continue;
        }
        let m_is_verify = WorkingMemory::step_requires_runtime_verification(m_trimmed);
        let required_overlap = m_tokens.len().div_ceil(2).max(1);
        let is_covered = (0..existing_count).any(|idx| {
            if m_is_verify && !WorkingMemory::step_requires_runtime_verification(&steps[idx]) {
                return false;
            }
            let subs_joined = step_substeps
                .get(idx)
                .map(|s| s.join(" "))
                .unwrap_or_default();
            let combined_lower = format!("{} {}", steps[idx], subs_joined).to_ascii_lowercase();
            let matched = m_tokens
                .iter()
                .filter(|tok| WorkingMemory::stem_token_matches_task(tok, &combined_lower))
                .count();
            matched >= required_overlap
        });
        if !is_covered {
            steps.push(m_trimmed.to_string());
            step_substeps.push(Vec::new());
        }
    }
}

/// Extracts macro step strings and per-step substeps from `create_plan` arguments while
/// preserving concrete file paths and `+`-separated deliverable identifiers when the caller
/// supplies `{ title, step, description }` objects (e.g., `step: "Build SiteHeader + HeroSection + TerminalPreview"`
/// or `step: "Create src/data/content.ts ..."` alongside `title` and `description`).
fn extract_plan_steps_and_substeps(
    args: &serde_json::Value,
) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let arr = ["steps", "tasks", "plan"]
        .iter()
        .find_map(|k| args.get(*k)?.as_array())?;

    let has_compound_ident = |tok: &str| -> bool {
        let clean = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.');
        if clean.len() < 4 || clean.contains(char::is_whitespace) {
            return false;
        }
        let has_dot_ext = clean.contains('.')
            && clean
                .rsplit_once('.')
                .is_some_and(|(stem, ext)| !stem.is_empty() && (1..=5).contains(&ext.len()));
        let has_camel = clean.chars().any(|c| c.is_ascii_lowercase())
            && clean.chars().skip(1).any(|c| c.is_ascii_uppercase());
        has_dot_ext || clean.contains('/') || has_camel
    };

    let mut steps = Vec::with_capacity(arr.len());
    let mut step_substeps = Vec::with_capacity(arr.len());

    for item in arr {
        if let Some(s) = item.as_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                steps.push(trimmed.to_string());
                step_substeps.push(Vec::new());
            }
            continue;
        }
        let Some(obj) = item.as_object() else {
            continue;
        };

        let title_opt = obj
            .get("title")
            .or_else(|| obj.get("name"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let action_opt = obj
            .get("step")
            .or_else(|| obj.get("task"))
            .or_else(|| obj.get("action"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let desc_opt = obj
            .get("description")
            .or_else(|| obj.get("details"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let head = match (title_opt, action_opt) {
            (Some(t), Some(a)) if !t.eq_ignore_ascii_case(a) => {
                let existing_combined = match desc_opt {
                    Some(d) => format!("{} {}", t, d),
                    None => t.to_string(),
                };
                let action_has_missing_deliverable = a.split_whitespace().any(|tok| {
                    let clean = tok
                        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '/');
                    has_compound_ident(tok) && !existing_combined.contains(clean)
                });
                if action_has_missing_deliverable {
                    format!("{}: {}", t, a)
                } else {
                    t.to_string()
                }
            }
            (Some(t), _) => t.to_string(),
            (None, Some(a)) => a.to_string(),
            (None, None) => match desc_opt {
                Some(d) => d.to_string(),
                None => continue,
            },
        };

        let macro_step = match desc_opt {
            Some(d) if head != d && !head.ends_with(d) => {
                if title_opt.is_some() && !head.contains(": ") {
                    format!("{}: {}", head, d)
                } else {
                    format!("{} — {}", head, d)
                }
            }
            _ => head,
        };

        let mut subs: Vec<String> = obj
            .get("substeps")
            .or_else(|| obj.get("sub_tasks"))
            .or_else(|| obj.get("subtasks"))
            .and_then(|v| v.as_array())
            .map(|sub_arr| {
                sub_arr
                    .iter()
                    .filter_map(|sub_item| {
                        sub_item.as_str().map(str::to_string).or_else(|| {
                            sub_item
                                .get("title")
                                .or_else(|| sub_item.get("step"))
                                .and_then(|t| t.as_str())
                                .map(str::to_string)
                        })
                    })
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // When no explicit `substeps` array was passed, decompose `+`-separated concrete code
        // deliverables (`2..=8` targets) structurally by target morphology rather than a hardcoded
        // English build-verb allowlist (supporting any non-operational verb or bare artifact list).
        if subs.is_empty() {
            if let Some(a) = action_opt {
                if !crate::context::working_memory::WorkingMemory::step_requires_runtime_verification(a)
                    && !a.contains(',')
                    && !a.contains(';')
                {
                    let plus_parts: Vec<&str> = a
                        .split(" + ")
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .collect();
                    if (2..=8).contains(&plus_parts.len()) {
                        let is_deliverable_target = |t: &str| -> bool {
                            if has_compound_ident(t) {
                                return true;
                            }
                            let clean =
                                t.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
                            clean.len() >= 4
                                && !clean.contains(char::is_whitespace)
                                && clean.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                                && clean.chars().skip(1).any(|c| c.is_ascii_lowercase())
                        };
                        let mut parsed_parts: Vec<(Option<&str>, &str)> =
                            Vec::with_capacity(plus_parts.len());
                        let mut all_valid = true;
                        for &seg in &plus_parts {
                            if is_deliverable_target(seg) {
                                parsed_parts.push((None, seg));
                                continue;
                            }
                            let words: Vec<&str> = seg.split_whitespace().collect();
                            if words.len() >= 2 {
                                let verb = words[0];
                                let rest = seg[verb.len()..].trim();
                                let verb_clean = verb
                                    .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
                                if !verb_clean.is_empty()
                                    && !has_compound_ident(verb)
                                    && !crate::context::working_memory::WorkingMemory::clause_starts_with_operational_verb(
                                        &verb_clean.to_ascii_lowercase(),
                                    )
                                    && is_deliverable_target(rest)
                                {
                                    parsed_parts.push((Some(verb), rest));
                                    continue;
                                }
                            }
                            all_valid = false;
                            break;
                        }
                        if all_valid
                            && parsed_parts.iter().any(|(_, tgt)| has_compound_ident(tgt))
                        {
                            let shared_verb = parsed_parts[0].0.unwrap_or("Build");
                            for (verb_opt, tgt) in parsed_parts {
                                let v = verb_opt.unwrap_or(shared_verb);
                                subs.push(format!("{} {}", v, tgt));
                            }
                        }
                    }
                }
            }
        }

        steps.push(macro_step);
        step_substeps.push(subs);
    }

    Some((steps, step_substeps))
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "remember_fact" => Some((|| {
            let key = param::require_str(args, "key", "remember_fact")?;
            let value = param::require_str(args, "value", "remember_fact")?;
            let is_global = param::opt_bool(args, "is_global", false);
            let cat_str = param::opt_str(args, "category").unwrap_or("project_fact");
            let category = match cat_str {
                "preference" => crate::context::memory::MemoryCategory::Preference,
                "pattern" => crate::context::memory::MemoryCategory::Pattern,
                _ => crate::context::memory::MemoryCategory::ProjectFact,
            };
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            if matches!(category, crate::context::memory::MemoryCategory::Preference) {
                let _ = crate::context::governance::dox::DoxEngine::record_user_preference(
                    workspace_root,
                    None,
                    &format!("{}: {}", key, value),
                );
            }
            mem.remember(workspace_root, key, value, is_global, category)
                .map(|_| {
                    format!(
                        "✔ Remembered '{}' ({})",
                        key,
                        if is_global { "global" } else { "local" }
                    )
                })
        })()),
        "update_fact" => Some((|| {
            let key = param::require_str(args, "key", "update_fact")?;
            let new_value =
                param::require_str_with_aliases(args, &["new_value", "value"], "update_fact")?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            let core_updated = mem.update(workspace_root, key, new_value)?;
            let mut prog =
                crate::context::progressive_memory::ProgressiveMemory::load(workspace_root);
            let prog_updated = prog.update_fact(key, new_value);
            if prog_updated {
                let _ = prog.save(workspace_root);
            }
            if core_updated || prog_updated {
                Ok(format!("✔ Updated fact '{}'", key))
            } else {
                Ok(format!("ℹ Fact '{}' not found to update", key))
            }
        })()),
        "forget_fact" => Some((|| {
            let key = param::require_str(args, "key", "forget_fact")?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            let core_forgotten = mem.forget(workspace_root, key)?;
            let mut prog =
                crate::context::progressive_memory::ProgressiveMemory::load(workspace_root);
            let prog_forgotten = prog.forget_fact(key);
            if prog_forgotten {
                let _ = prog.save(workspace_root);
            }
            if core_forgotten || prog_forgotten {
                Ok(format!("✔ Removed fact '{}' from memory", key))
            } else {
                Ok(format!("ℹ Fact '{}' not found in memory", key))
            }
        })()),
        "create_plan" => Some(
            async {
                let normalized_args =
                    crate::agent::providers::normalize_xml_gateway_wrappers(args.clone());
                let args = &normalized_args;
                let milestones = param::opt_string_array(args, "milestones")
                    .or_else(|| param::opt_string_array(args, "roadmap"));
                // Plan review runs before the plan is written; structural blockers keep the
                // discovery write-guard active unless `force=true`.
                let resolved_title = resolve_plan_title(args, workspace_root);
                let mut extracted_plan = extract_plan_steps_and_substeps(args);
                if let Some((ref mut s, ref mut subs)) = extracted_plan {
                    reconcile_steps_with_milestones(s, subs, milestones.as_deref());
                }
                let review_steps = extracted_plan
                    .as_ref()
                    .map(|(s, _)| s.clone())
                    .or_else(|| param::opt_string_array(args, "steps"))
                    .or_else(|| param::opt_string_array(args, "tasks"))
                    .or_else(|| param::opt_string_array(args, "plan"))
                    .unwrap_or_default();
                let review = if review_steps.is_empty() {
                    None
                } else {
                    let input = crate::agent::minipower::plan_review::PlanReviewInput {
                        topic: &resolved_title,
                        steps: &review_steps,
                        target_files: &[],
                        test_command: None,
                    };
                    match crate::agent::minipower::MiniPowerEngine::review_plan(
                        workspace_root,
                        input,
                        param::opt_bool(args, "force", false),
                    )
                    .await
                    {
                        Ok(report) => Some(report),
                        Err(blocked) => return Ok(blocked),
                    }
                };
                (|| -> Result<String> {
            let title = resolved_title.as_str();
            let (mut steps, mut step_substeps) = match extracted_plan {
                Some((s, subs)) => (s, subs),
                None => {
                    let mut s = param::opt_string_array(args, "steps")
                        .or_else(|| param::opt_string_array(args, "tasks"))
                        .or_else(|| param::opt_string_array(args, "plan"))
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "create_plan".to_string(),
                            reason: "Missing required argument 'steps' (or 'tasks')".to_string(),
                        })?;
                    let mut subs = vec![Vec::new(); s.len()];
                    reconcile_steps_with_milestones(&mut s, &mut subs, milestones.as_deref());
                    (s, subs)
                }
            };
            if steps.is_empty() {
                return Err(ToolError::InvalidArguments {
                    name: "create_plan".to_string(),
                    reason: "Argument 'steps' must contain at least one task step (e.g. ['Step 1', 'Step 2'] or [{'title': 'Step 1', 'description': '...'}]). Empty plans are not allowed.".to_string(),
                }.into());
            }
            reconcile_steps_with_milestones(&mut steps, &mut step_substeps, milestones.as_deref());
            let architecture = param::opt_str(args, "architecture")
                .or_else(|| param::opt_str(args, "blueprint"))
                .or_else(|| param::opt_str(args, "implementation"));
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            let _ = std::fs::remove_file(wm.last_verify_path());
            step_substeps.resize_with(steps.len(), Vec::new);
            for (idx, raw_step) in steps.iter().enumerate() {
                if step_substeps[idx].is_empty() {
                    let hybrid_subs = crate::context::working_memory::WorkingMemory::synthesize_hybrid_assembly_verification_substeps(raw_step);
                    if !hybrid_subs.is_empty() {
                        step_substeps[idx] = hybrid_subs;
                    } else {
                        let auto_subs = crate::context::working_memory::WorkingMemory::synthesize_compound_verification_substeps(raw_step);
                        if !auto_subs.is_empty() {
                            step_substeps[idx] = auto_subs;
                        } else {
                            let deliv_subs = crate::context::working_memory::WorkingMemory::synthesize_compound_deliverable_substeps(raw_step);
                            if !deliv_subs.is_empty() {
                                step_substeps[idx] = deliv_subs;
                            }
                        }
                    }
                } else if crate::context::working_memory::WorkingMemory::step_requires_runtime_verification(raw_step)
                    && step_substeps[idx].last().is_some_and(|last_sub| {
                        crate::context::working_memory::WorkingMemory::is_remediation_fix_step(
                            last_sub,
                        )
                    })
                {
                    step_substeps[idx].push(
                        "Run final regression build, lint, and pre-completion verification"
                            .to_string(),
                    );
                }
            }
            let _ = std::fs::remove_file(workspace_root.join(".minicode/plan/last_verify.json"));
            let _ = std::fs::remove_file(workspace_root.join(".minicode/plan/last_review.json"));
            wm.init_two_tier_plan_with_substeps(
                title,
                &steps,
                &step_substeps,
                architecture,
                milestones.as_deref(),
            )?;
            let assumptions = param::opt_string_array(args, "assumptions").unwrap_or_default();
            if let Err(e) =
                crate::agent::decisions::record_assumptions(workspace_root, title, &assumptions)
            {
                tracing::warn!("Failed to record plan assumptions: {}", e);
            }
            let first_step = steps
                .first()
                .map(|s| crate::context::working_memory::WorkingMemory::sanitize_step_title(s))
                .unwrap_or_else(|| "First step".to_string());
            let triad_note = review
                .as_ref()
                .map(|r| {
                    crate::agent::minipower::MiniPowerEngine::mark_plan_validated(
                        workspace_root,
                        title,
                        steps.len(),
                        r,
                    )
                })
                .unwrap_or_default();
            let _ =
                crate::agent::boulder::BoulderState::sync_from_workspace(workspace_root, false, 3);
            let impl_rel = wm
                .canonical_implementation_path()
                .strip_prefix(workspace_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| wm.canonical_implementation_path().display().to_string());
            let todo_rel = wm
                .canonical_todo_path()
                .strip_prefix(workspace_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| wm.canonical_todo_path().display().to_string());
            let plan_rel = wm
                .task_plan_path()
                .strip_prefix(workspace_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| wm.task_plan_path().display().to_string());
            let active_step_label = wm
                .read_active_plan_summary()
                .and_then(|s| s.active_task)
                .unwrap_or(first_step);
            Ok(format!(
                "✔ Created Two-Tier Plan for \"{}\":\n• Tier 1 Blueprint: {}\n• Tier 1 Roadmap: {} (view with /todo)\n• Tier 2 Active Steps: {} ({} macro step(s), live in TUI){}\n► Active Step: [>] \"{}\". Focus ONLY on executing this step now. (If this step is complex, you can decompose it into sub-steps with `power_plan(steps=[...])`; when finished, mark it done with `update_progress`.)",
                title,
                impl_rel,
                todo_rel,
                plan_rel,
                steps.len(),
                triad_note,
                active_step_label,
            ))
        })()
            }
            .await,
        ),
        "read_plan" => Some({
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            match wm.read_plan() {
                Ok(Some(plan)) => Ok(plan),
                Ok(None) => Ok("ℹ No active task plan found in .minicode/plan/".to_string()),
                Err(e) => Err(e),
            }
        }),
        "log_finding" => Some((|| {
            let finding = param::require_str(args, "finding", "log_finding")?;
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.append_finding(finding)
                .map(|_| "✔ Logged observation into .minicode/plan/findings.md".to_string())
        })()),
        "record_decision" => Some((|| {
            let id_or_title = param::require_str_with_aliases(
                args,
                &["id_or_title", "title", "id", "topic", "category", "name"],
                "record_decision",
            )?;
            let decision = param::require_str_with_aliases(
                args,
                &["decision", "value", "summary", "content"],
                "record_decision",
            )?;
            let rationale = param::opt_str(args, "rationale");
            let assigned_id = crate::agent::decisions::upsert_decision(
                workspace_root,
                id_or_title,
                decision,
                rationale,
            )?;
            Ok(format!(
                "✔ Recorded decision [{}] in .minicode/decisions.md (in-place Living Decision Ledger)",
                assigned_id
            ))
        })()),
        "update_progress" => Some((|| {
            let status = param::require_str_with_aliases(
                args,
                &["status", "state", "progress"],
                "update_progress",
            )?;
            let step_list: Vec<String> =
                if let Some(arr) = param::opt_string_array(args, "steps").filter(|a| !a.is_empty())
                {
                    arr
                } else if let Some(single) = param::get_str_with_aliases(
                    args,
                    &["step", "completed_step", "task", "id", "index"],
                )
                .map(str::trim)
                .filter(|s| !s.is_empty())
                {
                    vec![single.to_string()]
                } else {
                    vec!["active".to_string()]
                };

            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            let mut resolved_titles = Vec::with_capacity(step_list.len());
            let mut any_modified = false;
            for s in &step_list {
                let (modified, title) = wm.update_progress_detailed(s, status)?;
                any_modified |= modified;
                resolved_titles.push(title);
            }
            let step_label = resolved_titles.join("', '");
            let remaining_hint = wm.format_remaining_tasks_hint();
            if let Some(summary) = wm.read_active_plan_summary() {
                let plan_progress = if summary.tasks.len() > 1
                    && summary.tasks.iter().any(|t| {
                        t.status == crate::context::working_memory::TaskItemStatus::InProgress
                            && !t.sub_tasks.is_empty()
                    }) {
                    let macro_done = summary
                        .tasks
                        .iter()
                        .filter(|t| {
                            t.status == crate::context::working_memory::TaskItemStatus::Completed
                        })
                        .count();
                    format!(
                        "{}/{} macro tasks completed (active step sub-plan: {}/{} sub-steps completed)",
                        macro_done,
                        summary.tasks.len(),
                        summary.completed_tasks,
                        summary.total_tasks
                    )
                } else {
                    format!(
                        "{}/{} tasks completed",
                        summary.completed_tasks, summary.total_tasks
                    )
                };
                if let Some(ref next_active) = summary.active_task {
                    if any_modified {
                        Ok(format!(
                            "✔ Updated step '{}' to '{}'. Current plan: {}.\n{}\n► Next Active Step: [>] \"{}\" (auto-promoted — do NOT call `update_progress` with `status=\"in_progress\"`). Focus ONLY on executing this step now (or decompose it via `power_plan` if multi-part).",
                            step_label, status, plan_progress, remaining_hint, next_active
                        ))
                    } else {
                        Ok(format!(
                            "✔ Step '{}' is already '{}' (auto-reconciled — active step [>] preserved; completing a step automatically promotes the next step, so never emit a separate `in_progress` call). Current plan: {}.\n{}\n► Next Active Step: [>] \"{}\". Focus ONLY on executing and verifying this step now.",
                            step_label, status, plan_progress, remaining_hint, next_active
                        ))
                    }
                } else if summary.completed_tasks == summary.total_tasks && summary.total_tasks > 0
                {
                    let verify_passed = std::fs::read_to_string(
                        workspace_root.join(".minicode/plan/last_verify.json"),
                    )
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .and_then(|v| v.get("passed").and_then(|b| b.as_bool()))
                    .unwrap_or(false);
                    let macro_total = summary.tasks.len().max(summary.total_tasks);
                    if verify_passed {
                        Ok(format!(
                            "✔ Updated step '{}' to '{}'. 🎉 All {} tasks in active plan completed (100%) and `power_verify` passed!\n► Next Step: Run `power_review` (if not yet reviewed) and call `archive_plan` before presenting your final summary.",
                            step_label, status, macro_total
                        ))
                    } else {
                        Ok(format!(
                            "✔ Updated step '{}' to '{}'. 🎉 All {} tasks in active plan completed (100%)!\n► Next Step: Run `power_review` (diff audit) and `power_verify` (4-Gate Pre-Completion Verification Barrier), then call `archive_plan` to finalize.",
                            step_label, status, macro_total
                        ))
                    }
                } else {
                    Ok(format!(
                        "✔ Updated step '{}' status to '{}'.\n{}",
                        step_label, status, remaining_hint
                    ))
                }
            } else {
                Ok(format!(
                    "✔ Updated step '{}' status to '{}'",
                    step_label, status
                ))
            }
        })()),
        "archive_plan" => Some({
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            let had_todo_roadmap = wm.canonical_todo_path().is_file();
            let pre_archive_tasks = wm.read_parsed_tasks();
            // Synchronize IntentLedger on disk before archiving the completed plan
            let intent_path =
                workspace_root.join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
            if intent_path.exists() {
                if let Ok(mut ledger) =
                    crate::context::memory::intent::IntentLedger::load_from_disk(&intent_path)
                {
                    if ledger.sync_with_working_memory(&wm, 0) > 0 {
                        let _ = ledger.save_to_disk(&intent_path);
                    }
                }
            }
            let _ = crate::agent::minipower::MiniPowerEngine::set_planning_phase_state(
                workspace_root,
                &crate::agent::minipower::PlanningPhaseState::Unlocked,
            );
            match wm.archive_plan() {
                Ok(Some(archive_path)) => {
                    crate::agent::boulder::BoulderState::clear(workspace_root);
                    let rel_archive = archive_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&archive_path)
                        .display();
                    let summary_tasks = if had_todo_roadmap {
                        let milestones = wm.read_roadmap_milestones();
                        milestones
                            .iter()
                            .find(|m| m.is_active)
                            .or_else(|| milestones.iter().rev().find(|m| !m.tasks.is_empty()))
                            .map(|m| m.tasks.clone())
                            .filter(|t| !t.is_empty())
                            .unwrap_or(pre_archive_tasks)
                    } else {
                        pre_archive_tasks
                    };
                    if summary_tasks.is_empty() {
                        Ok(format!("✔ Archived completed task plan to {}", rel_archive))
                    } else {
                        let done_count = summary_tasks
                            .iter()
                            .filter(|t| {
                                t.status
                                    == crate::context::working_memory::TaskItemStatus::Completed
                            })
                            .count();
                        let total_count = summary_tasks.len();
                        let mut checklist = Vec::with_capacity(total_count.min(10) + 2);
                        checklist.push(format!(
                            "✔ Archived completed task plan ({}/{} steps completed) -> {}",
                            done_count, total_count, rel_archive
                        ));
                        for (idx, task) in summary_tasks.iter().take(10).enumerate() {
                            let short: String = if task.title.chars().count() > 78 {
                                let head: String = task.title.chars().take(78).collect();
                                format!("{}…", head.trim_end())
                            } else {
                                task.title.clone()
                            };
                            checklist.push(format!("  ✓ {}. {}", idx + 1, short));
                        }
                        if total_count > 10 {
                            checklist.push(format!(
                                "  ✓ … +{} additional completed step(s)",
                                total_count - 10
                            ));
                        }
                        Ok(checklist.join("\n"))
                    }
                }
                Ok(None) => Ok("ℹ No active task plan to archive".to_string()),
                Err(e) => Err(e),
            }
        }),
        "checkpoint_session" => Some((|| {
            let label = param::require_str(args, "label", "checkpoint_session")?;
            let description = param::opt_str(args, "description");

            let info = crate::context::checkpoint::SessionCheckpointer::create_checkpoint(
                workspace_root,
                "current_session",
                label,
                description,
                &[],
            )?;

            Ok(format!(
                "✔ Created session checkpoint `{}` (`{}`)\n- **Timestamp:** {}\n- **Working Memory Saved:** {}",
                info.label,
                info.id,
                info.timestamp,
                if info.has_working_plan { "Yes" } else { "No" }
            ))
        })()),
        "rewind_session" => Some((|| {
            let action = param::opt_str(args, "action").unwrap_or("list");

            match action {
                "list" => {
                    let list = crate::context::checkpoint::SessionCheckpointer::list_checkpoints(
                        workspace_root,
                        None,
                    )?;
                    if list.is_empty() {
                        return Ok("*(No checkpoints recorded for this workspace yet)*".to_string());
                    }
                    let mut out =
                        format!("# ⏱️ Workspace Session Checkpoints ({})\n\n", list.len());
                    out.push_str("| Checkpoint ID | Label | Timestamp | Plan Saved |\n");
                    out.push_str("| :--- | :--- | :--- | :--- |\n");
                    for ckpt in list {
                        out.push_str(&format!(
                            "| `{}` | **{}** | {} | {} |\n",
                            ckpt.id,
                            ckpt.label,
                            ckpt.timestamp,
                            if ckpt.has_working_plan { "✔" } else { "—" }
                        ));
                    }
                    Ok(out)
                }
                "rewind" => {
                    let ckpt_id = param::require_str(args, "checkpoint_id", "rewind_session")?;
                    let (report, _) =
                        crate::context::checkpoint::SessionCheckpointer::rewind_checkpoint(
                            workspace_root,
                            "current_session",
                            ckpt_id,
                        )?;
                    Ok(report.format_markdown())
                }
                "fork" => {
                    let ckpt_id = param::require_str(args, "checkpoint_id", "rewind_session")?;
                    let fork_label = param::opt_str(args, "fork_label");
                    let forked = crate::context::checkpoint::SessionCheckpointer::fork_checkpoint(
                        workspace_root,
                        ckpt_id,
                        fork_label,
                    )?;
                    Ok(format!(
                        "✔ Successfully forked checkpoint `{}` into new branch `{}` (`{}`)",
                        ckpt_id, forked.label, forked.id
                    ))
                }
                other => Err(ToolError::InvalidArguments {
                    name: "rewind_session".to_string(),
                    reason: format!("Unknown action: {}", other),
                }
                .into()),
            }
        })()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minitest59_resolve_plan_title_from_decisions_intent_and_architecture() {
        let temp = tempfile::tempdir().expect("tempdir");

        // 1. Empty workspace + omitted title falls back cleanly to "Task Plan"
        assert_eq!(
            resolve_plan_title(&serde_json::json!({"steps": ["Step 1"]}), temp.path()),
            "Task Plan"
        );

        // 2. Architecture opening clause beats generic "Task Plan" when no decisions/intent exist
        assert_eq!(
            resolve_plan_title(
                &serde_json::json!({
                    "title": "Task Plan",
                    "architecture": "# Swiss Editorial Landing Architecture — tokens, hero, and benchmark\nDetails...",
                    "steps": ["Step 1"]
                }),
                temp.path()
            ),
            "Swiss Editorial Landing Architecture"
        );

        // 3. `.minicode/decisions.md` heading (`## [D1] minicode landing page — foundation (...)`)
        //    is inferred cleanly when `title` is omitted or generic `"Task Plan"`.
        let minicode_dir = temp.path().join(".minicode");
        std::fs::create_dir_all(&minicode_dir).expect("create .minicode");
        std::fs::write(
            minicode_dir.join("decisions.md"),
            "# Living Decision Ledger\n\n## [D1] minicode landing page — foundation (answered by user, 2026-11-13 12:21 UTC)\n- **Decision**: Technical Swiss dark theme\n",
        )
        .expect("write decisions.md");

        assert_eq!(
            resolve_plan_title(&serde_json::json!({"steps": ["Step 1"]}), temp.path()),
            "minicode landing page"
        );
        assert_eq!(
            resolve_plan_title(
                &serde_json::json!({"title": "Task Plan", "steps": ["Step 1"]}),
                temp.path()
            ),
            "minicode landing page"
        );
    }

    #[tokio::test]
    async fn test_minitest59_redundant_in_progress_after_completed_returns_auto_reconciled() {
        let temp = tempfile::tempdir().expect("tempdir");
        let minicode_dir = temp.path().join(".minicode");
        std::fs::create_dir_all(&minicode_dir).expect("create .minicode");
        std::fs::write(
            minicode_dir.join("decisions.md"),
            "# Living Decision Ledger\n\n## [D1] minicode landing page — foundation (answered by user, 2026-11-13 12:21 UTC)\n- **Decision**: Technical Swiss dark theme\n",
        )
        .expect("write decisions.md");

        let created = dispatch(
            "create_plan",
            &serde_json::json!({
                "steps": [
                    "Foundation tokens and typography",
                    "Hero section implementation",
                    "Production build verification"
                ],
                "force": true
            }),
            temp.path(),
        )
        .await
        .expect("create_plan handled")
        .expect("create_plan ok");
        assert!(created.contains("Created Two-Tier Plan for \"minicode landing page\""));
        assert!(temp.path().join(".minicode/intent_anchor.json").exists());

        let step1_done = dispatch(
            "update_progress",
            &serde_json::json!({"step": "1", "status": "completed"}),
            temp.path(),
        )
        .await
        .expect("update_progress handled")
        .expect("update_progress ok");
        assert!(step1_done.contains("✔ Updated step"));
        assert!(step1_done.contains("auto-promoted"));

        // Redundant `in_progress` on Step 2 right after auto-promotion must report `(auto-reconciled`
        // even when `.minicode/intent_anchor.json` exists on disk.
        let redundant = dispatch(
            "update_progress",
            &serde_json::json!({"step": "2", "status": "in_progress"}),
            temp.path(),
        )
        .await
        .expect("update_progress handled")
        .expect("update_progress ok");
        assert!(
            redundant.contains("(auto-reconciled"),
            "Expected auto-reconciled no-op output, got:\n{}",
            redundant
        );
    }

    #[tokio::test]
    async fn test_minitest62_create_plan_preserves_step_field_and_synthesizes_all_substeps() {
        let temp = tempfile::tempdir().expect("tempdir");
        let payload = serde_json::json!({
            "title": "minicode premium landing page",
            "force": true,
            "steps": [
                {
                    "title": "Foundation: tokens, fonts, base CSS",
                    "step": "Install fonts + lucide-react, define Carbon Slate + Burnt Orange tokens in src/index.css, clean index.html + RootLayout",
                    "description": "Install @fontsource-variable/inter, @fontsource-variable/space-grotesk, @fontsource-variable/jetbrains-mono, lucide-react. Rewrite src/index.css with Swiss grid tokens. Update index.html meta. Replace RootLayout shell."
                },
                {
                    "title": "Typed content + facts module",
                    "step": "Create src/data/content.ts with all verified minicode facts, benchmarks, DAG nodes, council models, tools, comparison rows",
                    "description": "All copy, benchmark configs, Swarm DAG nodes, Council perspectives, PageRank symbols, security layers, 150+ tools categories, TUI vs NDJSON samples, comparison matrix, principles."
                },
                {
                    "title": "Header + Hero + Live Terminal",
                    "step": "Build SiteHeader + HeroSection + TerminalPreview",
                    "description": "Exposed hairline header with GitHub badge + install pill. Asymmetric 60/40 hero (h1 <= 2 lines, <= 10 words) + interactive live terminal simulator cycling real agent runs."
                },
                {
                    "title": "Philosophy + Live Performance Configurator",
                    "step": "Build PhilosophySection + PerformanceSection",
                    "description": "'Evidence Before Assertions' visual pipeline (Plan -> Inspect -> Act -> Verify -> Report) + interactive benchmark configurator comparing minicode vs Node/Python."
                },
                {
                    "title": "Swarm DAG + Multi-Agent Council",
                    "step": "Build SwarmSection + CouncilSection",
                    "description": "Interactive Swarm DAG orchestrator -> 4 worktrees -> verify -> merge + Multi-Agent Council 4-model disagreement synthesis."
                },
                {
                    "title": "Code Intelligence + Security by Design",
                    "step": "Build CodeIntelSection + SecuritySection",
                    "description": "AST/CodeGraph + PageRank centrality explorer + Security layers + live secret redaction demo."
                },
                {
                    "title": "Browser automation HUD + 150+ tools",
                    "step": "Build BrowserSection + ToolsSection",
                    "description": "Browser: mock browser chrome, gliding cursor, active-agent indicator, observability log. Tools: filterable 150+ tools across 8 categories."
                },
                {
                    "title": "Interfaces, comparison, principles, social proof, CTA, footer",
                    "step": "Build InterfacesSection + ComparisonSection + PrinciplesSection + SocialProofSection + ClosingCta + Footer",
                    "description": "Interfaces: interactive TTY vs headless stream toggle with JSON preview. Comparison: honest matrix vs Claude Code/Cursor/Aider/Codex CLI. Principles: 6 cards. Social proof: GitHub stars/version/OSS/metrics. Closing CTA + colophon footer."
                },
                {
                    "title": "Assembly + verification",
                    "step": "Wire HomePage, build, lint, start dev server, browser verify 1440/768/375, fix any defects",
                    "description": "Wire all sections into HomePage, run tsc + eslint + vite build, start dev server, screenshot 1440/768/375, inspect layout/overflow/console/interactions, remediate."
                }
            ]
        });

        let created = dispatch("create_plan", &payload, temp.path())
            .await
            .expect("create_plan handled")
            .expect("create_plan ok");
        assert!(created.contains("Created Two-Tier Plan"));

        let wm = crate::context::working_memory::WorkingMemory::new(temp.path());
        let tasks = wm.read_roadmap_milestones()[0].tasks.clone();
        assert_eq!(tasks.len(), 9);

        // Step 1: 3 imperative foundation substeps (`Rewrite src/index.css...`, `Update index.html...`, `Replace RootLayout...`)
        assert_eq!(
            tasks[0].sub_tasks.len(),
            3,
            "Step 1 subs: {:?}",
            tasks[0].sub_tasks
        );
        // Step 2: preserves `src/data/content.ts` from `step` field!
        assert!(
            tasks[1].title.contains("src/data/content.ts"),
            "Step 2 title lost `src/data/content.ts`: {}",
            tasks[1].title
        );
        // Step 3: 3 substeps (`SiteHeader`, `HeroSection`, `TerminalPreview`)
        assert_eq!(tasks[2].sub_tasks.len(), 3);
        assert_eq!(tasks[2].sub_tasks[0].title, "Build SiteHeader");
        assert_eq!(tasks[2].sub_tasks[1].title, "Build HeroSection");
        assert_eq!(tasks[2].sub_tasks[2].title, "Build TerminalPreview");
        // Steps 4..=7: 2 component substeps each
        assert_eq!(tasks[3].sub_tasks.len(), 2);
        assert_eq!(tasks[4].sub_tasks.len(), 2);
        assert_eq!(tasks[5].sub_tasks.len(), 2);
        assert_eq!(tasks[6].sub_tasks.len(), 2);
        // Step 8: 6 component substeps
        assert_eq!(tasks[7].sub_tasks.len(), 6);
        assert_eq!(tasks[7].sub_tasks[0].title, "Build InterfacesSection");
        assert_eq!(tasks[7].sub_tasks[5].title, "Build Footer");
        // Step 9: 5 hybrid assembly + verification substeps
        assert_eq!(
            tasks[8].sub_tasks.len(),
            5,
            "Step 9 subs: {:?}",
            tasks[8].sub_tasks
        );
        assert!(tasks[8].sub_tasks[0].title.contains("HomePage"));

        // Verb-agnostic & bare-target `+` decomposition in `extract_plan_steps_and_substeps`
        let custom_extracted = extract_plan_steps_and_substeps(&serde_json::json!({
            "steps": [
                { "title": "Cards & Modals", "step": "Craft AlphaCard + BetaModal + GammaDrawer" },
                { "title": "Core Modules", "step": "src/a.ts + src/b.ts" }
            ]
        }))
        .expect("extracted");
        assert_eq!(
            custom_extracted.1[0],
            vec!["Craft AlphaCard", "Craft BetaModal", "Craft GammaDrawer"]
        );
        assert_eq!(
            custom_extracted.1[1],
            vec!["Build src/a.ts", "Build src/b.ts"]
        );
    }

    #[tokio::test]
    async fn test_minitest63_exact_create_plan_payload_recovers_full_6_milestone_roadmap_and_root_title(
    ) {
        let temp = tempfile::tempdir().expect("tempdir");

        // Seed pre-plan IntentLedger (16 prompt bullets + root_objective) exactly as in minitest63
        let initial_ledger = crate::context::memory::intent::IntentLedger::from_prompt(
            "Create a premium, modern landing page for minicode, an autonomous AI coding agent.\n\n- Extremely lightweight resource footprint\n- Fast startup and execution\n- Autonomous multi-agent swarm workflows",
            16,
        );
        let intent_path = temp
            .path()
            .join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
        initial_ledger
            .save_to_disk(&intent_path)
            .expect("save initial ledger");

        // Exact L68 payload from minitest63 session JSONL (hijacked `title`, 1-element `steps`
        // missing `title`, spilled `item` with nested `substeps: {"item": [...]}` + 6 `milestones`)
        let payload = serde_json::json!({
            "force": true,
            "architecture": "Single-page React 19 + Vite 8 + Tailwind 4 landing page for minicode. Dark technical aesthetic, bento workbench grid, live interactive micro-tool in hero.",
            "assumptions": [
                "Dark-only visual theme (developer tooling aesthetic), no light-mode toggle",
                "GitHub link points to https://github.com/aswin402/minicode"
            ],
            "milestones": [
                "Foundation: React 19 + Vite + Tailwind 4 scaffold with minicode design tokens",
                "Hero + nav with live interactive terminal product preview",
                "Core narrative sections: philosophy, lightweight architecture, code intelligence, security",
                "Interactive visual sections: autonomous swarm DAG, multi-agent council, browser HUD",
                "Ecosystem sections: 150+ tools catalog, interfaces (terminal + headless streaming), comparison, principles, social proof, CTA, footer",
                "Responsive verification at 1440/768/390 + build verification"
            ],
            "steps": [
                {
                    "description": "Scaffold react-vite stack via kit_stack_add, install deps, set up Tailwind 4 tokens, fonts, global CSS, base layout shell.",
                    "substeps": [
                        "Run kit_stack_add(stack_name=\"react-vite\")",
                        "Add lucide-react dependency",
                        "Define CSS variables (--bg, --surface-1/2, --border, --accent, --text) in src/index.css",
                        "Wire fonts (Space Grotesk + Inter + JetBrains Mono) with font-display swap"
                    ]
                }
            ],
            "title": "Scaffold React+Vite+Tailwind foundation with design tokens",
            "item": {
                "description": "Sticky top nav, 2-line hero headline, subcopy, Get Started + View on GitHub CTAs, and a live terminal preview showing scripted agent activity.",
                "substeps": {
                    "item": [
                        "Nav.tsx with GitHub link and install command",
                        "Hero.tsx with headline, CTAs, install command block",
                        "TerminalPreview.tsx: scripted timeline, type-on log lines, status chips, running/verified states",
                        "data/terminalScript.ts with realistic agent transcript steps"
                    ]
                },
                "title": "Build nav + hero with interactive terminal preview"
            }
        });

        let created = dispatch("create_plan", &payload, temp.path())
            .await
            .expect("create_plan handled")
            .expect("create_plan ok");
        assert!(
            !created.contains("Created Two-Tier Plan for \"Scaffold React+Vite+Tailwind"),
            "Root plan title must NOT be hijacked by Step 1's title:\n{}",
            created
        );

        let wm = crate::context::working_memory::WorkingMemory::new(temp.path());
        let milestones = wm.read_roadmap_milestones();
        assert_eq!(milestones.len(), 1);
        let tasks = &milestones[0].tasks;
        assert_eq!(
            tasks.len(),
            6,
            "All 6 project milestones must be present in todo.md, got: {:?}",
            tasks.iter().map(|t| &t.title).collect::<Vec<_>>()
        );
        assert!(tasks[0]
            .title
            .contains("Scaffold React+Vite+Tailwind foundation with design tokens"));
        assert_eq!(tasks[0].sub_tasks.len(), 4);
        assert!(tasks[1]
            .title
            .contains("Build nav + hero with interactive terminal preview"));
        assert_eq!(tasks[1].sub_tasks.len(), 4);
        assert!(tasks[2].title.contains("Core narrative sections"));
        assert!(tasks[3].title.contains("Interactive visual sections"));
        assert!(tasks[4].title.contains("Ecosystem sections"));
        assert!(tasks[5].title.contains("Responsive verification"));

        // Persisted IntentLedger preserves root_objective and aligns 1-to-1 with the 6 plan steps
        // (Step 1 InProgress, 0/6 Completed) instead of 16 stale prompt bullets!
        let synced_ledger =
            crate::context::memory::intent::IntentLedger::load_from_disk(&intent_path)
                .expect("load synced ledger");
        assert!(synced_ledger
            .root_objective
            .starts_with("Create a premium, modern landing page for minicode"));
        assert_eq!(synced_ledger.items.len(), 6);
        assert_eq!(synced_ledger.completed_count(), 0);
        assert_eq!(
            synced_ledger.items[0].status,
            crate::context::memory::intent::RequirementStatus::InProgress
        );

        // `/todo` modal (`ModalState::new_todo`) displays a single clean active roadmap milestone
        // (no duplicate Goal Anchor task-mirror clutter, and never `100% [Done]`)
        if let crate::ui::modals::ModalState::Todo(todo_modal) =
            crate::ui::modals::ModalState::new_todo(temp.path())
        {
            assert_eq!(todo_modal.milestones.len(), 1);
            assert!(todo_modal.milestones[0].is_active);
            assert_eq!(todo_modal.milestones[0].tasks.len(), 6);
        } else {
            panic!("Expected ModalState::Todo");
        }
    }
}
