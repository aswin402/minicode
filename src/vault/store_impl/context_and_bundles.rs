//! Dynamic context matching, progressive disclosure prompt assembly, and skill bundles for [`VaultStore`].

use super::*;
use crate::vault::builtin::get_all_builtin_bundles;
use crate::vault::models::{
    sanitize_skill_name, Result, SkillKind, SkillScope, VaultBundle, VaultError, VaultSkill,
};
use std::fs;
use std::path::Path;

impl VaultStore {
    // =========================================================================
    // Dynamic Context Matching & Prompt Assembly Engine (Zero-Hardcoding)
    // =========================================================================

    /// Scans workspace manifests and configuration files to extract a normalized set of technology package tokens.
    /// Delegates to [`WorkspaceProfile::extract_stack_tokens`] so manifest rules remain unified across the engine.
    pub fn detect_workspace_package_tokens(&self) -> std::collections::HashSet<String> {
        crate::context::workspace_profile::WorkspaceProfile::extract_stack_tokens(
            &self.workspace_root,
        )
    }

    /// Evaluates if a skill matches the workspace stack based on token-set intersection
    /// between the skill's identifier tokens (`skill.name`) and detected workspace package tokens.
    pub fn matches_skill_to_stack(
        &self,
        skill: &VaultSkill,
        stack_tokens: &std::collections::HashSet<String>,
    ) -> bool {
        if stack_tokens.is_empty() || skill.category().eq_ignore_ascii_case("efficiency") {
            return false;
        }
        let name_lower = skill.name.to_lowercase();
        if stack_tokens.contains(&name_lower) {
            return true;
        }

        // Extract non-version identifier tokens from skill name (e.g. "tailwind-v4" -> ["tailwind"],
        // "rust-tokio" -> ["rust", "tokio"], "slack-gif-creator" -> ["slack", "gif", "creator"]).
        let name_tokens: Vec<&str> = name_lower
            .split('-')
            .filter(|part| {
                part.len() >= 3
                    && !(part.starts_with('v') && part[1..].chars().all(|c| c.is_ascii_digit()))
            })
            .collect();

        if name_tokens.is_empty() {
            return false;
        }

        let token_in_stack = |tok: &str| -> bool {
            let tok_stem = tok
                .strip_suffix("js")
                .or_else(|| tok.strip_suffix("css"))
                .unwrap_or(tok);
            stack_tokens.contains(tok)
                || stack_tokens.contains(tok_stem)
                || (tok_stem.len() >= 3
                    && stack_tokens.iter().any(|st| {
                        let st_stem = st
                            .strip_suffix("js")
                            .or_else(|| st.strip_suffix("css"))
                            .unwrap_or(st.as_str());
                        st_stem == tok_stem
                    }))
        };

        // Reference and Runtime skills match when any primary technology token in their name is
        // present in `stack_tokens`; Workflow/Core skills require all name tokens to match.
        match skill.kind() {
            SkillKind::Runtime | SkillKind::Reference => {
                name_tokens.iter().any(|tok| token_in_stack(tok))
            }
            SkillKind::Workflow | SkillKind::Core => {
                name_tokens.iter().all(|tok| token_in_stack(tok))
            }
        }
    }

    /// Synchronizes workspace-applicable skills (stack dependencies, concrete file globs, and
    /// `always_apply` orchestrator skills) into `minikit.json` / `onpkg.json` (`active_skills`).
    /// Returns the updated list of active skill names in the project.
    pub fn sync_project_active_skills<P: AsRef<Path>>(&self, workspace_files: &[P]) -> Vec<String> {
        let all_skills = self.list_all_skills();
        let stack_tokens = self.detect_workspace_package_tokens();
        let project_skills_dir = self.project_skills_dir();

        let mut desired: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        for skill in &all_skills {
            // Skip on-disk minikit_docs/core/ and minikit_docs/packages/ files (which are tracked under their own doc sections)
            let is_ondisk_doc = skill.kind() == SkillKind::Core
                || skill.path.as_ref().is_some_and(|p| {
                    p.components()
                        .any(|c| matches!(c.as_os_str().to_str(), Some("packages" | "core")))
                });
            if is_ondisk_doc {
                continue;
            }

            let is_efficiency = skill.category().eq_ignore_ascii_case("efficiency");
            let explicitly_installed_on_disk = project_skills_dir.join(&skill.name).is_dir()
                || project_skills_dir
                    .join(format!("{}.md", skill.name))
                    .is_file();

            let should_activate = if is_efficiency {
                explicitly_installed_on_disk
            } else {
                skill.is_active_in_project
                    || skill.frontmatter.always_apply
                    || self.matches_skill_to_stack(skill, &stack_tokens)
                    || (!workspace_files.is_empty()
                        && workspace_files
                            .iter()
                            .any(|f| skill.matches_file(&self.workspace_root, f.as_ref())))
            };

            if should_activate {
                desired.insert(skill.name.to_lowercase());
            }
        }

        let desired_vec: Vec<String> = desired.into_iter().collect();
        let _ = self.mutate_manifest_active_skills(|arr| {
            let current: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_lowercase()))
                .collect();
            if current == desired_vec {
                false
            } else {
                *arr = desired_vec
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect();
                true
            }
        });

        desired_vec
    }

    fn matches_declared_repo_state(
        skill: &VaultSkill,
        repo_state: Option<crate::agent::orchestrator::RepoState>,
    ) -> Option<bool> {
        if skill.frontmatter.when_repo_state.is_empty() {
            return None;
        }
        let rs = repo_state?;
        let state_slug = match rs {
            crate::agent::orchestrator::RepoState::FreshWorkspace => "fresh",
            crate::agent::orchestrator::RepoState::DocsOnly => "docs_only",
            crate::agent::orchestrator::RepoState::ExistingCodebase => "existing",
        };
        Some(
            skill
                .frontmatter
                .when_repo_state
                .iter()
                .any(|w| w.trim().eq_ignore_ascii_case(state_slug)),
        )
    }

    fn score_skill_for_context<P: AsRef<Path>>(
        &self,
        skill: &VaultSkill,
        stack_tokens: &std::collections::HashSet<String>,
        repo_state: Option<crate::agent::orchestrator::RepoState>,
        prompt_lower: Option<&str>,
        objective_lower: Option<&str>,
        active_files: &[P],
    ) -> usize {
        let is_efficiency = skill.category().eq_ignore_ascii_case("efficiency");
        let mut score = 0usize;

        let prompt_score = prompt_lower.map_or(0, |p| skill.prompt_match_score(p));
        let repo_state_match = Self::matches_declared_repo_state(skill, repo_state);
        if repo_state_match == Some(false) && prompt_score < 85 && !skill.is_active_in_project {
            return 0;
        }

        if skill.is_active_in_project {
            score = score.saturating_add(100);
        }
        if skill.frontmatter.always_apply {
            score = score.saturating_add(90);
        }
        if repo_state_match == Some(true) {
            score = score.saturating_add(35);
        }
        if prompt_score >= 45 {
            score = score.saturating_add(40usize.saturating_add(prompt_score));
        }
        if let Some(obj) = objective_lower {
            let obj_score = skill.prompt_match_score(obj);
            if obj_score >= 45 {
                score = score.saturating_add(30usize.saturating_add(obj_score / 2));
            }
        }
        if !is_efficiency && self.matches_skill_to_stack(skill, stack_tokens) {
            score = score.saturating_add(50);
        }
        if !is_efficiency
            && !active_files.is_empty()
            && active_files
                .iter()
                .any(|f| skill.matches_file(&self.workspace_root, f.as_ref()))
        {
            score = score.saturating_add(40);
        }

        score
    }

    fn match_from_loaded_skills<P: AsRef<Path>>(
        &self,
        all_skills: &[VaultSkill],
        stack_tokens: &std::collections::HashSet<String>,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<VaultSkill> {
        let prompt_lower = prompt.map(|p| p.to_lowercase());
        let objective_lower = session_objective.map(|o| o.to_lowercase());
        let (repo_state, _, _) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);

        let mut scored: Vec<(usize, VaultSkill)> = all_skills
            .iter()
            .filter_map(|skill| {
                let score = self.score_skill_for_context(
                    skill,
                    stack_tokens,
                    Some(repo_state),
                    prompt_lower.as_deref(),
                    objective_lower.as_deref(),
                    active_files,
                );
                (score > 0).then(|| (score, skill.clone()))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        scored.dedup_by(|a, b| a.1.name == b.1.name);
        scored.into_iter().map(|(_, s)| s).collect()
    }

    /// Matches skills for the current context (stack tokens, active files, user prompt, and session objective).
    /// Purely metadata-driven: checks frontmatter `always_apply`, `globs`, `triggers`, and project scope.
    pub fn match_skills_for_context<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<VaultSkill> {
        let all_skills = self.list_all_skills();
        let stack_tokens = self.detect_workspace_package_tokens();
        self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        )
    }

    /// Resolves the specific domain and workflow skills actively matched for this turn
    /// based on prompt intent, session objective, and active file paths.
    pub fn resolve_turn_active_skills<P: AsRef<Path>>(
        &self,
        prompt: &str,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<String> {
        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return Vec::new();
        }
        let stack_tokens = self.detect_workspace_package_tokens();
        let mut ranked: Vec<(usize, String)> = all_skills
            .into_iter()
            .filter(|s| {
                matches!(
                    s.kind(),
                    SkillKind::Runtime | SkillKind::Reference | SkillKind::Workflow
                )
            })
            .filter_map(|skill| {
                let prompt_score = skill.prompt_match_score(prompt);
                let is_package_doc = skill.kind() == SkillKind::Reference
                    && skill
                        .path
                        .as_ref()
                        .is_some_and(|p| p.components().any(|c| c.as_os_str() == "packages"));
                // Auto-generated `minikit_docs/packages/` stubs are passive API references;
                // only surface them as active turn skills when directly queried in the user prompt.
                if is_package_doc && prompt_score < 45 && !skill.is_active_in_project {
                    return None;
                }
                let obj_score = session_objective
                    .map(|obj| skill.prompt_match_score(obj) / 2)
                    .unwrap_or(0);
                let is_efficiency = skill.category().eq_ignore_ascii_case("efficiency");
                let active_project_bonus = if skill.is_active_in_project {
                    60usize
                } else {
                    0usize
                };
                let stack_score =
                    if !is_efficiency && self.matches_skill_to_stack(&skill, &stack_tokens) {
                        50usize
                    } else {
                        0usize
                    };
                let file_score = if !active_files.is_empty()
                    && !is_efficiency
                    && active_files
                        .iter()
                        .any(|f| skill.matches_file(&self.workspace_root, f.as_ref()))
                {
                    50usize
                } else {
                    0usize
                };
                let total = prompt_score
                    .max(obj_score)
                    .saturating_add(active_project_bonus)
                    .saturating_add(stack_score)
                    .saturating_add(file_score);
                (total >= 45).then_some((total, skill.name))
            })
            .collect();

        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        ranked.dedup_by(|a, b| a.1 == b.1);
        ranked.into_iter().take(4).map(|(_, name)| name).collect()
    }

    /// Formats progressive agent prompt context using Anthropic/Hermes 3-Level Progressive Disclosure:
    /// - Level 1: Compact metadata index (`name`, `description`, `path` / `triggers`) for local package references and available vault skills.
    /// - Level 2: Full guidelines for concise active stack/workflow skills within a bounded token budget; executive summary + on-demand pointer (`vault_show` / `read_file`) for large skill manuals.
    /// - Level 3: Bundled `scripts/`, `references/`, and `assets/` paths surfaced for on-demand inspection.
    pub fn format_prompt_skills_context<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> String {
        let max_inline_skill_chars = crate::constants::VAULT_MAX_INLINE_SKILL_CHARS;
        let total_level2_budget_chars = prompt
            .map_or(crate::constants::VAULT_STATIC_LEVEL2_BUDGET_CHARS, |_| {
                crate::constants::VAULT_DYNAMIC_LEVEL2_BUDGET_CHARS
            });
        let executive_summary_chars = crate::constants::VAULT_EXECUTIVE_SUMMARY_CHARS;

        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return String::new();
        }

        let stack_tokens = self.detect_workspace_package_tokens();
        let matched = self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        );
        let matched_names: std::collections::HashSet<String> =
            matched.iter().map(|s| s.name.to_lowercase()).collect();

        let mut out = String::new();
        let mut used_level2_chars: usize = 0;

        // 1. Core Project Invariants & Specifications (Authoritative)
        let max_core_spec_inline_chars = crate::constants::VAULT_MAX_CORE_SPEC_INLINE_CHARS;
        let core_specs: Vec<_> = matched
            .iter()
            .filter(|s| s.kind() == SkillKind::Core)
            .collect();
        if !core_specs.is_empty() {
            out.push_str("\n# Core Project Invariants & Specifications (Authoritative):\n");
            out.push_str("The following project specifications define the non-negotiable contract and architecture for this workspace:\n\n");
            for s in core_specs {
                let body = s.instructions.trim();
                if body.len() <= max_core_spec_inline_chars {
                    out.push_str(&format!("## Core Spec: `{}`\n{}\n\n", s.name, body));
                } else {
                    let cutoff = body.floor_char_boundary(max_core_spec_inline_chars);
                    let path_hint = s
                        .path
                        .as_ref()
                        .and_then(|p| {
                            p.strip_prefix(&self.workspace_root)
                                .ok()
                                .or(Some(p.as_path()))
                        })
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| format!("{}.md", s.name));
                    out.push_str(&format!(
                        "## Core Spec: `{}`\n{}\n\n> Full specification continues in `{}` — call `read_file(\"{}\")` to inspect remaining sections.\n\n",
                        s.name,
                        body[..cutoff].trim(),
                        path_hint,
                        path_hint
                    ));
                }
            }
        }

        // 2. Runtime & Architecture Workflow Conventions
        let runtime_workflows: Vec<_> = matched
            .iter()
            .filter(|s| s.kind() == SkillKind::Runtime)
            .collect();
        if !runtime_workflows.is_empty() {
            out.push_str("\n# Project Architecture & Runtime Workflow Conventions:\n");
            out.push_str("The workspace runtime conventions are active. Adhere strictly to these patterns:\n\n");
            for r in runtime_workflows {
                out.push_str(&format!(
                    "## Runtime `{}`\n_{}_\n\n{}\n\n",
                    r.name,
                    r.description,
                    r.instructions.trim()
                ));
            }
        }

        // Helper to determine if a Reference skill is an on-disk `minikit_docs/packages/<pkg>.md` manual
        let is_package_manual = |s: &VaultSkill| -> bool {
            s.kind() == SkillKind::Reference
                && s.path
                    .as_ref()
                    .is_some_and(|p| p.components().any(|c| c.as_os_str() == "packages"))
                && s.instructions.len() > crate::constants::VAULT_PACKAGE_MANUAL_MIN_CHARS
        };

        // 3. Active Technology & Methodology Skills (prioritized first in Level-2 budget) & 4. Active Procedural & Behavioral Skills
        for (kind, heading, subheading) in [
            (
                SkillKind::Reference,
                "\n# Active Technology & Methodology Skills:\n",
                "The following domain skills are actively applied to this workspace/task. Adhere strictly to their guidelines and invariants:\n\n",
            ),
            (
                SkillKind::Workflow,
                "\n# Active Procedural & Behavioral Skills:\n",
                "The following engineering methodologies govern execution quality for this workspace:\n\n",
            ),
        ] {
            let group: Vec<_> = matched
                .iter()
                .filter(|s| s.kind() == kind && !is_package_manual(s))
                .collect();

            if !group.is_empty() {
                out.push_str(heading);
                out.push_str(subheading);
                for skill in group {
                    let body = skill.instructions.trim();
                    let bundled = skill.discover_bundled_resources();
                    let bundled_line = if !bundled.is_empty() {
                        format!(
                            "\n_Level-3 Bundled Resources: {}_\n",
                            bundled.join(", ")
                        )
                    } else {
                        String::new()
                    };

                    let kind_budget = match kind {
                        SkillKind::Reference => crate::constants::VAULT_DYNAMIC_LEVEL2_BUDGET_CHARS,
                        _ => total_level2_budget_chars,
                    };

                    if body.len() <= max_inline_skill_chars
                        && used_level2_chars.saturating_add(body.len()) <= kind_budget
                    {
                        used_level2_chars = used_level2_chars.saturating_add(body.len());
                        out.push_str(&format!(
                            "## Skill `{}` ({})\n_{}_{}\n\n{}\n\n",
                            skill.name,
                            skill.category(),
                            skill.description,
                            bundled_line,
                            body
                        ));
                    } else {
                        let preview_end = body
                            .char_indices()
                            .take_while(|(idx, _)| *idx < executive_summary_chars)
                            .last()
                            .map(|(idx, ch)| idx + ch.len_utf8())
                            .unwrap_or(0);
                        let preview = body[..preview_end].trim();
                        let path_hint = skill
                            .path
                            .as_ref()
                            .and_then(|p| p.strip_prefix(&self.workspace_root).ok().or(Some(p.as_path())))
                            .map(|p| format!(" or `read_file(\"{}\")`", p.display()))
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "## Skill `{}` ({}) [Progressive Level-2 Summary]\n_{}_{}\n\n{}\n\n> Full specification available on-demand via `vault_show(\"{}\")`{}.\n\n",
                            skill.name,
                            skill.category(),
                            skill.description,
                            bundled_line,
                            preview,
                            skill.name,
                            path_hint
                        ));
                    }
                }
            }
        }

        // 5. Local Package References & Continuous Learning Protocol (Level 1 Index -> Level 3 On-Demand)
        let package_manuals: Vec<_> = matched.iter().filter(|s| is_package_manual(s)).collect();
        if !package_manuals.is_empty() {
            out.push_str("\n# Local Package References & Self-Improving Knowledge Base:\n");
            out.push_str("Installed Workspace Package Manuals (inspect on-demand via `read_file` or `kit_skill_show(name)`):\n");
            for pkg in package_manuals {
                let rel_path = pkg
                    .path
                    .as_ref()
                    .and_then(|p| p.strip_prefix(&self.workspace_root).ok())
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| format!("minikit_docs/packages/{}.md", pkg.name));
                out.push_str(&format!(
                    "  • `{}` (`{}`): {}\n",
                    pkg.name, rel_path, pkg.description
                ));
            }
            out.push('\n');
        }

        // Section 6: Compact Category-Grouped Directory of Available Skills (Level 1 Metadata Index)
        let mut by_category: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for s in all_skills
            .into_iter()
            .filter(|s| !matched_names.contains(&s.name.to_lowercase()))
        {
            by_category
                .entry(s.category().to_lowercase())
                .or_default()
                .push(format!("`{}`", s.name));
        }

        if !by_category.is_empty() {
            out.push_str("# Available Domain Skills in Vault:\n");
            out.push_str("<available_skills>\n");
            for (cat, mut names) in by_category {
                names.sort();
                out.push_str(&format!(
                    "  • {} ({}): {}\n",
                    cat,
                    names.len(),
                    names.join(", ")
                ));
            }
            out.push_str("</available_skills>\n");
            out.push_str("<skills_guidance>\n");
            out.push_str("Call `vault_show(name=\"<skill>\")` (or `kit_skill_show(name=\"<skill>\")`) to load full Level-2 instructions or `vault_show(name=\"<skill>\", file_path=\"references/...\")` for Level-3 bundled resources.\n");
            out.push_str("</skills_guidance>\n\n");
        }

        out
    }

    /// Collects the lowercase skill names that are inlined in full into the static baseline system prompt.
    #[must_use]
    pub fn collect_baseline_inlined_skill_names(&self) -> Vec<String> {
        let max_inline_skill_chars = crate::constants::VAULT_MAX_INLINE_SKILL_CHARS;
        let all_skills = self.list_all_skills();
        let stack_tokens = self.detect_workspace_package_tokens();
        let (_, mut workspace_files, doc_files) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);
        workspace_files.extend(doc_files);
        self.match_from_loaded_skills(&all_skills, &stack_tokens, None, None, &workspace_files)
            .into_iter()
            .filter(|s| s.instructions.trim().len() <= max_inline_skill_chars)
            .map(|s| s.name.to_lowercase())
            .collect()
    }

    /// Formats dynamic turn-level skill activations for Hermes agent orchestration.
    /// Emits `<auto_activated_skills>` if new skills match the turn prompt or modified files,
    /// or if a large skill summarized in the static prompt is directly triggered by the current turn.
    pub fn format_dynamic_turn_skills<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
        exclude_skills: &[String],
    ) -> Option<String> {
        let max_inline_skill_chars = crate::constants::VAULT_MAX_INLINE_SKILL_CHARS;

        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return None;
        }
        let stack_tokens = self.detect_workspace_package_tokens();

        let mut exclude_set: std::collections::HashSet<String> =
            exclude_skills.iter().map(|s| s.to_lowercase()).collect();

        // When no frozen session-baseline exclude set is provided, compute the current baseline inlined skills
        if exclude_set.is_empty() {
            for baseline_skill in
                self.match_from_loaded_skills::<&str>(&all_skills, &stack_tokens, None, None, &[])
            {
                if baseline_skill.instructions.trim().len() <= max_inline_skill_chars {
                    exclude_set.insert(baseline_skill.name.to_lowercase());
                }
            }
        }

        let matched = self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        );
        let new_skills: Vec<_> = matched
            .into_iter()
            .filter(|s| {
                let matches_turn_intent = [prompt, session_objective]
                    .into_iter()
                    .flatten()
                    .any(|text| s.prompt_match_score(text) >= 45);
                if s.frontmatter.always_apply && exclude_set.contains(&s.name.to_lowercase()) {
                    return matches_turn_intent;
                }
                matches_turn_intent || !exclude_set.contains(&s.name.to_lowercase())
            })
            .take(6)
            .collect();

        if new_skills.is_empty() {
            return None;
        }

        let mut out = String::from("<auto_activated_skills>\n");
        out.push_str("  Hermes Progressive Skill Intelligence Activated:\n");
        out.push_str("  The following domain skills automatically matched your prompt intent or active files:\n");

        for skill in new_skills {
            out.push_str(&format!(
                "  • Skill `{}` ({}): {}\n",
                skill.name,
                skill.category(),
                skill.description
            ));
            let bundled = skill.discover_bundled_resources();
            if !bundled.is_empty() {
                if let Some(ref skill_path) = skill.path {
                    if let Some(skill_dir) = skill_path.parent() {
                        out.push_str(&format!(
                            "    Bundled Level-3 Resources in `{}` (read via `vault_show(name=\"{}\", file_path=\"...\")`): {}\n",
                            skill_dir.display(),
                            skill.name,
                            bundled.join(", ")
                        ));
                    }
                }
            }
            out.push_str("    Guidelines & Invariants:\n");
            for line in skill.instructions.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("---") {
                    out.push_str(&format!("      {}\n", trimmed));
                }
            }
            out.push('\n');
        }

        out.push_str(
            "  Directive: Adhere strictly to these procedural skill guidelines for this task.\n",
        );
        out.push_str("</auto_activated_skills>");
        Some(out)
    }

    /// Collects all `allowed_tools` (tool names or category identifiers) declared in the YAML frontmatter
    /// of skills currently active for the workspace, prompt, or active file set.
    pub fn collect_active_skill_allowed_tools<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<String> {
        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return Vec::new();
        }
        let stack_tokens = self.detect_workspace_package_tokens();
        let matched = self.match_from_loaded_skills(
            &all_skills,
            &stack_tokens,
            prompt,
            session_objective,
            active_files,
        );
        let mut allowed = Vec::new();
        for skill in matched {
            for tool_name in skill.frontmatter.allowed_tools {
                let trimmed = tool_name.trim().to_string();
                if !trimmed.is_empty() && !allowed.contains(&trimmed) {
                    allowed.push(trimmed);
                }
            }
        }
        allowed
    }

    /// Returns skills whose frontmatter `after_tools` declaratively hooks into `tool_name`,
    /// filtered by live `when_repo_state` if declared.
    #[must_use]
    pub fn match_post_tool_skills(&self, tool_name: &str) -> Vec<VaultSkill> {
        let canonical = crate::tools::meta::canonical_name(tool_name);
        let (repo_state, _, _) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);
        self.list_all_skills()
            .into_iter()
            .filter(|skill| {
                let hooks_tool = skill.frontmatter.after_tools.iter().any(|t| {
                    let tc = crate::tools::meta::canonical_name(t.trim());
                    tc.eq_ignore_ascii_case(canonical) || t.trim().eq_ignore_ascii_case(tool_name)
                });
                hooks_tool
                    && Self::matches_declared_repo_state(skill, Some(repo_state)) != Some(false)
            })
            .collect()
    }

    /// Formats a concise `<post_tool_skill_guidance>` transition block from skills declaring
    /// `after_tools: [<tool_name>]` in their YAML frontmatter.
    #[must_use]
    pub fn format_post_tool_skills(&self, tool_name: &str) -> Option<(String, Vec<VaultSkill>)> {
        let matched = self.match_post_tool_skills(tool_name);
        if matched.is_empty() {
            return None;
        }
        let mut out = format!("<post_tool_skill_guidance tool=\"{}\">\n", tool_name);
        for skill in &matched {
            out.push_str(&format!(
                "  • Next-Phase Skill `{}` ({}): {}\n",
                skill.name,
                skill.category(),
                skill.description
            ));
        }
        out.push_str("</post_tool_skill_guidance>");
        Some((out, matched))
    }

    // =========================================================================
    // MiniVault Bundles Engine (Skill Packs / Stacks)
    // =========================================================================

    /// Discovers all skill bundles across all tiers (Built-in, Global, Project).
    pub fn list_all_bundles(&self) -> Vec<VaultBundle> {
        let mut bundle_map: std::collections::BTreeMap<String, VaultBundle> =
            std::collections::BTreeMap::new();

        // 1. Built-in Core Bundles (Tier 1)
        for b in get_all_builtin_bundles() {
            bundle_map.insert(b.name.to_lowercase(), b);
        }

        // 2. Global User Bundles (Tier 2)
        if let Some(global_dir) = self.global_bundles_dir() {
            self.scan_bundles_directory(&global_dir, SkillScope::Global, &mut bundle_map);
        }

        // 3. Project Bundles (Tier 3)
        let proj_dir = self.project_bundles_dir();
        self.scan_bundles_directory(&proj_dir, SkillScope::Project, &mut bundle_map);

        bundle_map.into_values().collect()
    }

    fn scan_bundles_directory(
        &self,
        dir: &Path,
        scope: SkillScope,
        map: &mut std::collections::BTreeMap<String, VaultBundle>,
    ) {
        if !dir.exists() {
            return;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut bundle) = serde_json::from_str::<VaultBundle>(&content) {
                            bundle.scope = scope;
                            bundle.path = Some(path);
                            map.insert(bundle.name.to_lowercase(), bundle);
                        }
                    }
                }
            }
        }
    }

    /// Finds a bundle by name across all tiers.
    pub fn get_bundle(&self, name: &str) -> Option<VaultBundle> {
        let clean = name.trim().to_lowercase();
        self.list_all_bundles()
            .into_iter()
            .find(|b| b.name.eq_ignore_ascii_case(&clean))
    }

    /// Loads all skills in a bundle into the active project (.minicode/skills/).
    /// Returns (loaded_skills, failed_skills).
    pub fn load_bundle_to_project(&self, bundle_name: &str) -> Result<(Vec<String>, Vec<String>)> {
        let bundle = self.get_bundle(bundle_name).ok_or_else(|| {
            VaultError::NotFound(format!("Bundle '{}' not found in vault", bundle_name))
        })?;

        let mut loaded = Vec::new();
        let mut failed = Vec::new();

        for skill_name in &bundle.skills {
            match self.load_to_project(skill_name) {
                Ok(_) => loaded.push(skill_name.clone()),
                Err(_) => failed.push(skill_name.clone()),
            }
        }

        Ok((loaded, failed))
    }

    /// Unloads all skills in a bundle from the active project.
    pub fn unload_bundle_from_project(&self, bundle_name: &str) -> Result<Vec<String>> {
        let bundle = self.get_bundle(bundle_name).ok_or_else(|| {
            VaultError::NotFound(format!("Bundle '{}' not found in vault", bundle_name))
        })?;

        let mut unloaded = Vec::new();
        for skill_name in &bundle.skills {
            if self.unload_from_project(skill_name).is_ok() {
                unloaded.push(skill_name.clone());
            }
        }
        Ok(unloaded)
    }

    /// Creates and saves a custom bundle definition as JSON in Project or Global scope.
    pub fn create_bundle(
        &self,
        scope: SkillScope,
        name: &str,
        description: &str,
        category: &str,
        skills: Vec<String>,
        tags: Vec<String>,
    ) -> Result<VaultBundle> {
        let clean_name = sanitize_skill_name(name);
        if clean_name.is_empty() {
            return Err(VaultError::InvalidSkill(
                "Bundle name cannot be empty".to_string(),
            ));
        }

        let target_dir = match scope {
            SkillScope::Project => self.project_bundles_dir(),
            SkillScope::Global => self.global_bundles_dir().ok_or_else(|| {
                VaultError::InvalidSkill(
                    "Cannot resolve user home directory for global vault".to_string(),
                )
            })?,
            SkillScope::Builtin => {
                return Err(VaultError::InvalidSkill(
                    "Cannot create custom bundles in immutable Builtin scope".to_string(),
                ));
            }
        };

        fs::create_dir_all(&target_dir).map_err(|e| VaultError::Io {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let file_path = target_dir.join(format!("{}.json", clean_name));
        let bundle = VaultBundle {
            name: clean_name.clone(),
            description: description.trim().to_string(),
            category: if category.trim().is_empty() {
                "general".to_string()
            } else {
                category.trim().to_lowercase()
            },
            skills,
            tags,
            scope,
            path: Some(file_path.clone()),
        };

        let json = serde_json::to_string_pretty(&bundle)
            .map_err(|e| VaultError::InvalidSkill(format!("Failed to serialize bundle: {}", e)))?;

        fs::write(&file_path, json).map_err(|e| VaultError::Io {
            path: file_path.display().to_string(),
            source: e,
        })?;

        Ok(bundle)
    }
}
