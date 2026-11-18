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

    /// Evaluates if a skill matches the workspace stack via pure set-theoretic inclusion
    /// ($S_{\text{id}}(d) \subseteq T_{\text{pkg}}$) in [`crate::vault::ranking::matches_stack_set_inclusion`].
    pub fn matches_skill_to_stack(
        &self,
        skill: &VaultSkill,
        stack_tokens: &std::collections::HashSet<String>,
    ) -> bool {
        crate::vault::ranking::matches_stack_set_inclusion(skill, stack_tokens)
    }

    /// Synchronizes workspace-applicable skills into `minikit.json` / `onpkg.json` (`active_skills`)
    /// using differential set algebra:
    /// $$U = (M_{\text{prev}} \setminus A_{\text{prev}}) \cup D_{\text{installed}}, \qquad M_{\text{now}} = (U \cup A_{\text{now}}) \setminus \text{Deleted}$$
    /// Preserves all user-pinned skills (regardless of category) while automatically pruning stale
    /// auto-detected skills when dependencies or files are removed from the workspace.
    pub fn sync_project_active_skills<P: AsRef<Path>>(&self, workspace_files: &[P]) -> Vec<String> {
        let all_skills = self.list_all_skills();
        let stack_tokens = self.detect_workspace_package_tokens();
        let deleted = self.load_deleted_skills();
        let project_skills_dir = self.project_skills_dir();
        let dot_skills_dir = self.workspace_root.join(crate::constants::SKILLS_DIR_NAME);

        let manifest_prev: std::collections::HashSet<String> =
            self.get_manifest_active_skills().into_iter().collect();
        let auto_ledger_path = self.project_auto_skills_file();
        let auto_prev = Self::read_tombstone_set(&auto_ledger_path);

        // User-pinned set: anything previously in manifest that wasn't merely auto-synced
        let mut user_pinned: std::collections::BTreeSet<String> = manifest_prev
            .difference(&auto_prev)
            .filter(|name| !deleted.contains(*name))
            .cloned()
            .collect();

        let code_workspace_files: Vec<&Path> = workspace_files
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| {
                let rel = p.strip_prefix(&self.workspace_root).unwrap_or(p);
                let rel_str = rel.to_string_lossy();
                let clean = rel_str.trim_start_matches("./");
                let ext = rel
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                !clean.starts_with("minikit_docs/")
                    && !clean.starts_with("onpkg_docs/")
                    && !clean.starts_with(".minicode/")
                    && !clean.starts_with(".git/")
                    && !matches!(ext.as_str(), "md" | "mdx" | "markdown" | "txt" | "lock")
            })
            .collect();

        let mut auto_now: std::collections::HashSet<String> = std::collections::HashSet::new();

        for skill in &all_skills {
            let lower = skill.name.to_lowercase();
            let is_ondisk_doc = skill.kind() == SkillKind::Core
                || lower.starts_with("minikit-")
                || lower.starts_with("pkg-")
                || skill.path.as_ref().is_some_and(|p| {
                    p.components()
                        .any(|c| matches!(c.as_os_str().to_str(), Some("packages" | "core")))
                });
            if is_ondisk_doc {
                continue;
            }

            let installed_on_disk = project_skills_dir.join(&skill.name).is_dir()
                || project_skills_dir
                    .join(format!("{}.md", skill.name))
                    .is_file()
                || dot_skills_dir.join(&skill.name).is_dir()
                || dot_skills_dir.join(format!("{}.md", skill.name)).is_file();

            if installed_on_disk {
                user_pinned.insert(lower.clone());
            }

            // Persistent workspace auto-sync pins:
            // 1. Standing `always_apply` invariants,
            // 2. Exact stack dependency set-inclusion (`S_id(d) \subseteq T_pkg`),
            // 3. Language/framework `Reference` or `Runtime` manuals matched by real source-code files
            //    (excluding episodic `Workflow` skills, which activate dynamically per-turn via BM25F).
            let stack_matched = skill.frontmatter.category.as_deref() != Some("efficiency")
                && crate::vault::ranking::matches_stack_set_inclusion(skill, &stack_tokens);
            let reference_glob_matched =
                matches!(skill.kind(), SkillKind::Reference | SkillKind::Runtime)
                    && skill.frontmatter.when_repo_state.is_empty()
                    && crate::vault::ranking::compute_file_glob_specificity(
                        skill,
                        &self.workspace_root,
                        &code_workspace_files,
                    ) > 0.0;
            let auto_matched =
                skill.frontmatter.always_apply || stack_matched || reference_glob_matched;

            if auto_matched && !deleted.contains(&lower) {
                auto_now.insert(lower);
            }
        }

        let mut desired: std::collections::BTreeSet<String> = user_pinned;
        for s in &auto_now {
            desired.insert(s.clone());
        }

        let has_manifest = ["minikit.json", "onpkg.json"]
            .into_iter()
            .any(|name| self.workspace_root.join(name).exists());
        if has_manifest
            || auto_ledger_path.exists()
            || !workspace_files.is_empty()
            || !stack_tokens.is_empty()
        {
            let _ = Self::write_tombstone_set(&auto_ledger_path, &auto_now);
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

    fn match_from_loaded_skills<P: AsRef<Path>>(
        &self,
        all_skills: &[VaultSkill],
        stack_tokens: &std::collections::HashSet<String>,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
    ) -> Vec<VaultSkill> {
        let (repo_state, _, _) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);
        let telemetry = self.load_skill_telemetry();
        let corpus = crate::vault::ranking::SkillCorpusIndex::build(all_skills);
        corpus
            .rank_all(
                &self.workspace_root,
                stack_tokens,
                Some(repo_state),
                prompt,
                session_objective,
                active_files,
                self.available_tools.as_ref(),
                self.active_tools.as_ref(),
                Some(&telemetry),
            )
            .into_iter()
            .map(|cand| cand.skill)
            .collect()
    }

    /// Matches skills for the current context (stack tokens, active files, user prompt, and session objective)
    /// using multi-field BM25F + set-theoretic stack & glob specificity ranking.
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
    /// via BM25F corpus ranking + stack inclusion + glob specificity + live tool capability gating.
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
        let (repo_state, _, _) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);
        let telemetry = self.load_skill_telemetry();
        let corpus = crate::vault::ranking::SkillCorpusIndex::build(&all_skills);
        corpus
            .rank_all(
                &self.workspace_root,
                &stack_tokens,
                Some(repo_state),
                Some(prompt),
                session_objective,
                active_files,
                self.available_tools.as_ref(),
                self.active_tools.as_ref(),
                Some(&telemetry),
            )
            .into_iter()
            .filter(|cand| {
                // Global `always_apply` baseline skills are already present in the static prompt;
                // only report them in turn-level `SkillActivity` when directly queried in `prompt`.
                if cand.skill.frontmatter.always_apply && cand.prompt_bm25 < 3.0 {
                    return false;
                }
                let domain_grounded = cand.matched_stack
                    || (cand.glob_specificity > 0.0
                        && (cand.skill.is_active_in_project || cand.lexical_score >= 1.8));
                matches!(
                    cand.skill.kind(),
                    SkillKind::Runtime | SkillKind::Reference | SkillKind::Workflow
                ) && (cand.lexical_score >= 2.8 || domain_grounded)
                    && cand.total_score >= 4.5
            })
            .take(4)
            .map(|cand| cand.skill.name)
            .collect()
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

        // Helper to determine if a Reference skill is an on-disk `minikit_docs/packages/<pkg>.md` manual
        let is_package_manual = |s: &VaultSkill| -> bool {
            s.kind() == SkillKind::Reference
                && s.path
                    .as_ref()
                    .is_some_and(|p| p.components().any(|c| c.as_os_str() == "packages"))
                && s.instructions.len() > crate::constants::VAULT_PACKAGE_MANUAL_MIN_CHARS
        };

        // 2, 3 & 4. Unified Progressive Level-2 Budget across Runtime, Reference & Workflow Skills
        for (kind, heading, subheading) in [
            (
                SkillKind::Runtime,
                "\n# Project Architecture & Runtime Workflow Conventions:\n",
                "The workspace runtime conventions are active. Adhere strictly to these patterns:\n\n",
            ),
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
                    let rendered = skill.render_instructions(&self.workspace_root);
                    let body = rendered.trim();
                    let bundled = skill.discover_bundled_resources();
                    let bundled_line = if !bundled.is_empty() {
                        format!(
                            "\n_Level-3 Bundled Resources: {}_\n",
                            bundled.join(", ")
                        )
                    } else {
                        String::new()
                    };

                    let is_fork =
                        skill.frontmatter.context_mode() == crate::vault::models::SkillContextMode::Fork;

                    if !is_fork
                        && body.len() <= max_inline_skill_chars
                        && used_level2_chars.saturating_add(body.len()) <= total_level2_budget_chars
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
                    } else if !is_fork
                        && used_level2_chars.saturating_add(executive_summary_chars)
                            <= total_level2_budget_chars.saturating_mul(2)
                    {
                        let preview_end = body
                            .char_indices()
                            .take_while(|(idx, _)| *idx < executive_summary_chars)
                            .last()
                            .map(|(idx, ch)| idx + ch.len_utf8())
                            .unwrap_or(0);
                        let preview = body[..preview_end].trim();
                        used_level2_chars = used_level2_chars.saturating_add(preview.len());
                        let path_hint = skill
                            .path
                            .as_ref()
                            .and_then(|p| {
                                p.strip_prefix(&self.workspace_root)
                                    .ok()
                                    .or(Some(p.as_path()))
                            })
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
                    } else {
                        let fork_hint = if is_fork {
                            let role = skill.frontmatter.agent.as_deref().unwrap_or("scout");
                            format!(
                                " [context: fork -> `delegate_task(role=\"{}\", load_skills=[\"{}\"])`]",
                                role, skill.name
                            )
                        } else {
                            format!(" [load: `vault_show(\"{}\")`]", skill.name)
                        };
                        out.push_str(&format!(
                            "• **`{}`** ({}) — {}{}\n",
                            skill.name,
                            skill.category(),
                            skill.description,
                            fork_hint
                        ));
                    }
                }
                out.push('\n');
            }
        }

        // 5. Local Package References & Continuous Learning Protocol (Level 1 Index -> Level 3 On-Demand)
        let package_manuals: Vec<_> = matched.iter().filter(|s| is_package_manual(s)).collect();
        if !package_manuals.is_empty() {
            out.push_str("\n# Local Package References & Self-Improving Knowledge Base:\n");
            out.push_str("Installed Workspace Package Manuals (inspect on-demand via `read_file` or `vault_show(name)`):\n");
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

        // Section 6: Ecosystem-Scoped Semantic Directory of Available Skills (Level 1 Metadata Index)
        let mut by_category: std::collections::BTreeMap<String, Vec<(String, String)>> =
            std::collections::BTreeMap::new();
        let mut hidden_foreign_refs = 0usize;
        for s in all_skills
            .into_iter()
            .filter(|s| !matched_names.contains(&s.name.to_lowercase()))
        {
            if !crate::vault::ranking::is_capability_eligible(
                &s,
                self.available_tools.as_ref(),
                self.active_tools.as_ref(),
            ) {
                continue;
            }
            // When the project has a concrete detected dependency set, omit disjoint foreign-stack
            // API Reference cheatsheets from the static prompt (keeping 100% searchable via `vault_search`).
            if !stack_tokens.is_empty() && s.kind() == SkillKind::Reference {
                let id_tokens = crate::vault::ranking::extract_skill_identifier_tokens(&s.name);
                if !id_tokens.is_empty() && id_tokens.iter().all(|tok| !stack_tokens.contains(tok))
                {
                    hidden_foreign_refs += 1;
                    continue;
                }
            }
            let short_desc = crate::utils::strings::truncate_chars(&s.description, 56).to_string();
            by_category
                .entry(s.category().to_lowercase())
                .or_default()
                .push((s.name, short_desc));
        }

        if !by_category.is_empty() || hidden_foreign_refs > 0 {
            out.push_str("# Available Domain Skills in Vault:\n");
            out.push_str("<available_skills>\n");
            for (cat, mut items) in by_category {
                items.sort_by(|a, b| a.0.cmp(&b.0));
                let formatted: Vec<String> = items
                    .into_iter()
                    .map(|(name, desc)| format!("`{}` ({})", name, desc))
                    .collect();
                out.push_str(&format!(
                    "  • {} ({}): {}\n",
                    cat,
                    formatted.len(),
                    formatted.join("; ")
                ));
            }
            if hidden_foreign_refs > 0 {
                out.push_str(&format!(
                    "  • cross-stack-references ({} omitted for token efficiency): query anytime via `vault_search(query=\"...\")`\n",
                    hidden_foreign_refs
                ));
            }
            out.push_str("</available_skills>\n");
            out.push_str("<skills_guidance>\n");
            out.push_str("Select relevant skills autonomously for the task: call `vault_show(name=\"<skill>\")` to load Level-2 instructions, `vault_show(name=\"<skill>\", file_path=\"references/...\")` for Level-3 bundled resources, or pass `load_skills: [\"<skill>\"]` to `delegate_task` for isolated subagent execution.\n");
            out.push_str("</skills_guidance>\n\n");
        }

        out
    }

    /// Collects the lowercase skill names that are actually inlined in full into the static baseline system prompt
    /// within `VAULT_STATIC_LEVEL2_BUDGET_CHARS`.
    #[must_use]
    pub fn collect_baseline_inlined_skill_names(&self) -> Vec<String> {
        let max_inline_skill_chars = crate::constants::VAULT_MAX_INLINE_SKILL_CHARS;
        let total_budget = crate::constants::VAULT_STATIC_LEVEL2_BUDGET_CHARS;
        let all_skills = self.list_all_skills();
        let stack_tokens = self.detect_workspace_package_tokens();
        let (_, mut workspace_files, doc_files) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);
        workspace_files.extend(doc_files);
        let matched =
            self.match_from_loaded_skills(&all_skills, &stack_tokens, None, None, &workspace_files);

        let mut used = 0usize;
        let mut inlined = Vec::new();
        for kind in [
            SkillKind::Runtime,
            SkillKind::Reference,
            SkillKind::Workflow,
        ] {
            for skill in matched.iter().filter(|s| s.kind() == kind) {
                let body_len = skill.instructions.trim().len();
                let is_fork = skill.frontmatter.context_mode()
                    == crate::vault::models::SkillContextMode::Fork;
                if !is_fork
                    && body_len <= max_inline_skill_chars
                    && used.saturating_add(body_len) <= total_budget
                {
                    used = used.saturating_add(body_len);
                    inlined.push(skill.name.to_lowercase());
                }
            }
        }
        inlined
    }

    /// Formats dynamic turn-level skill activations and returns `(rendered_block, newly_inlined_skill_names)`
    /// so `AgentLoop` can record newly inlined skills into `session_active_skills` and never re-dump them
    /// on subsequent turns.
    pub fn format_dynamic_turn_skills_with_names<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
        exclude_skills: &[String],
    ) -> Option<(String, Vec<String>)> {
        let max_inline_skill_chars = crate::constants::VAULT_MAX_INLINE_SKILL_CHARS;
        let total_dynamic_budget = crate::constants::VAULT_DYNAMIC_LEVEL2_BUDGET_CHARS;

        let all_skills = self.list_all_skills();
        if all_skills.is_empty() {
            return None;
        }
        let stack_tokens = self.detect_workspace_package_tokens();
        let (repo_state, _, _) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);

        let mut exclude_set: std::collections::HashSet<String> =
            exclude_skills.iter().map(|s| s.to_lowercase()).collect();

        if exclude_set.is_empty() {
            for name in self.collect_baseline_inlined_skill_names() {
                exclude_set.insert(name);
            }
        }

        let telemetry = self.load_skill_telemetry();
        let corpus = crate::vault::ranking::SkillCorpusIndex::build(&all_skills);
        let partition = corpus.partition_turn_skills(
            &self.workspace_root,
            &stack_tokens,
            Some(repo_state),
            prompt,
            session_objective,
            active_files,
            &exclude_set,
            self.available_tools.as_ref(),
            self.active_tools.as_ref(),
            Some(&telemetry),
            max_inline_skill_chars,
            total_dynamic_budget,
        );

        if partition.inline_skills.is_empty() && partition.candidate_slate.is_empty() {
            return None;
        }

        let mut newly_inlined_names = Vec::new();
        let mut out = String::from("<auto_activated_skills>\n");
        out.push_str("  Hermes Progressive Skill Intelligence Activated:\n");
        let mut used_dynamic_chars = 0usize;
        if !partition.inline_skills.is_empty() {
            out.push_str("  BM25F High-Confidence Active Skills (auto-matched for this turn):\n");
            for cand in &partition.inline_skills {
                let skill = &cand.skill;
                newly_inlined_names.push(skill.name.to_lowercase());
                let conf_pct = (cand.confidence * 100.0).round() as usize;
                out.push_str(&format!(
                    "  • Skill `{}` ({} | relevance: {}%): {}\n",
                    skill.name,
                    skill.category(),
                    conf_pct,
                    skill.description
                ));
                let bundled = skill.discover_bundled_resources();
                if !bundled.is_empty() {
                    let skill_dir = skill.base_dir(&self.workspace_root);
                    out.push_str(&format!(
                        "    Bundled Level-3 Resources in `{}` (read via `vault_show(name=\"{}\", file_path=\"...\")`): {}\n",
                        skill_dir.display(),
                        skill.name,
                        bundled.join(", ")
                    ));
                }
                out.push_str("    Guidelines & Invariants:\n");
                let rendered = skill.render_instructions(&self.workspace_root);
                let trimmed_body = rendered.trim();
                let remaining = total_dynamic_budget.saturating_sub(used_dynamic_chars);
                let allowed_for_this = max_inline_skill_chars.min(remaining.max(1_200));
                let (slice_body, was_truncated) = if trimmed_body.len() > allowed_for_this {
                    let end = trimmed_body.floor_char_boundary(allowed_for_this);
                    (&trimmed_body[..end], true)
                } else {
                    (trimmed_body, false)
                };
                used_dynamic_chars = used_dynamic_chars.saturating_add(slice_body.len());
                for line in slice_body.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with("---") {
                        out.push_str(&format!("      {}\n", trimmed));
                    }
                }
                if was_truncated {
                    out.push_str(&format!(
                        "      > [Progressive Level-2 Cap: call `vault_show(name=\"{}\")` to inspect remaining sections.]\n",
                        skill.name
                    ));
                }
                out.push('\n');
            }
        }

        if !partition.candidate_slate.is_empty() {
            out.push_str("  Orchestrator Candidate Skill Slate (select & load on-demand if useful for this task):\n");
            for cand in &partition.candidate_slate {
                let skill = &cand.skill;
                let conf_pct = (cand.confidence * 100.0).round() as usize;
                let bundled = skill.discover_bundled_resources();
                let res_note = if bundled.is_empty() {
                    String::new()
                } else {
                    format!(" [resources: {}]", bundled.join(", "))
                };
                if skill.frontmatter.context_mode() == crate::vault::models::SkillContextMode::Fork
                {
                    let role = skill.frontmatter.agent.as_deref().unwrap_or("scout");
                    out.push_str(&format!(
                        "  • `{}` ({} | {}% | context: fork): {}{} -> Run isolated via `delegate_task(role=\"{}\", load_skills=[\"{}\"], task=\"...\")`.\n",
                        skill.name,
                        skill.category(),
                        conf_pct,
                        skill.description,
                        res_note,
                        role,
                        skill.name
                    ));
                } else {
                    out.push_str(&format!(
                        "  • `{}` ({} | {}%): {}{} -> Inspect via `vault_show(name=\"{}\")`.\n",
                        skill.name,
                        skill.category(),
                        conf_pct,
                        skill.description,
                        res_note,
                        skill.name
                    ));
                }
            }
        }

        out.push_str(
            "  Directive: Apply inlined skill invariants and call `vault_show` (or `delegate_task` with `load_skills`) for any candidate skill needed by your plan.\n",
        );
        out.push_str("</auto_activated_skills>");
        Some((out, newly_inlined_names))
    }

    /// Formats dynamic turn-level skill activations using BM25F + Candidate-Slate-First + Single-Pin partitioning.
    pub fn format_dynamic_turn_skills<P: AsRef<Path>>(
        &self,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
        exclude_skills: &[String],
    ) -> Option<String> {
        self.format_dynamic_turn_skills_with_names(
            prompt,
            session_objective,
            active_files,
            exclude_skills,
        )
        .map(|(block, _)| block)
    }

    /// Collects all explicit non-wildcard `allowed_tools` declared on a sticky set of session-active skill names.
    pub fn collect_allowed_tools_for_skill_names<I, S>(&self, skill_names: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let targets: std::collections::HashSet<String> = skill_names
            .into_iter()
            .map(|s| sanitize_skill_name(s.as_ref()))
            .filter(|s| !s.is_empty())
            .collect();
        if targets.is_empty() {
            return Vec::new();
        }
        let mut allowed = Vec::new();
        for skill in self.list_all_skills() {
            if targets.contains(&skill.name.to_lowercase()) {
                for tool_name in skill.frontmatter.allowed_tools {
                    let trimmed = tool_name.trim().to_string();
                    if !trimmed.is_empty()
                        && trimmed != "*"
                        && !trimmed.eq_ignore_ascii_case("all")
                        && !allowed.contains(&trimmed)
                    {
                        allowed.push(trimmed);
                    }
                }
            }
        }
        allowed
    }

    /// Collects all explicit non-wildcard `allowed_tools` declared in the YAML frontmatter
    /// of skills actively ranked for the workspace, prompt, or active file set.
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
        let (repo_state, _, _) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(&self.workspace_root);
        let telemetry = self.load_skill_telemetry();
        let corpus = crate::vault::ranking::SkillCorpusIndex::build(&all_skills);
        let ranked = corpus.rank_all(
            &self.workspace_root,
            &stack_tokens,
            Some(repo_state),
            prompt,
            session_objective,
            active_files,
            self.available_tools.as_ref(),
            self.active_tools.as_ref(),
            Some(&telemetry),
        );
        let mut allowed = Vec::new();
        for cand in ranked
            .into_iter()
            .filter(|c| {
                c.total_score >= 4.2
                    && (c.lexical_score >= 2.6
                        || (c.skill.scope == SkillScope::Project && c.skill.is_active_in_project))
            })
            .take(2)
        {
            for tool_name in cand.skill.frontmatter.allowed_tools {
                let trimmed = tool_name.trim().to_string();
                if !trimmed.is_empty()
                    && trimmed != "*"
                    && !trimmed.eq_ignore_ascii_case("all")
                    && !allowed.contains(&trimmed)
                {
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

    /// Finds workspace or global `MiniVault` domain skills matching a navigated URL's hostname
    /// (`browser-harness` `BH_DOMAIN_SKILLS` auto-surfacing, closing the Skill Forge loop).
    #[must_use]
    pub fn find_domain_skills_for_url(&self, raw_url: &str) -> Vec<(String, String)> {
        let Ok(parsed) = url::Url::parse(raw_url) else {
            return Vec::new();
        };
        let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else {
            return Vec::new();
        };
        if matches!(
            host.as_str(),
            "localhost" | "127.0.0.1" | "::1" | "0.0.0.0" | "example.com"
        ) {
            return Vec::new();
        }
        let apex = {
            let parts: Vec<&str> = host.split('.').collect();
            if parts.len() >= 2 {
                format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
            } else {
                host.clone()
            }
        };

        let mut matches = Vec::new();
        for skill in self.list_all_skills() {
            if skill.scope == SkillScope::Builtin || skill.frontmatter.always_apply {
                continue;
            }
            let name_lc = skill.name.to_ascii_lowercase();
            let trigger_hit = skill.frontmatter.triggers.iter().any(|t| {
                let tl = t.trim().to_ascii_lowercase();
                tl == host || tl == apex || (tl.contains('.') && host.ends_with(&tl))
            });
            let host_stem = apex.split('.').next().unwrap_or("");
            let slug_hit = host_stem.len() >= 4
                && skill.category().eq_ignore_ascii_case("browser")
                && name_lc.contains(host_stem);
            if trigger_hit || slug_hit {
                matches.push((skill.name, skill.description));
                if matches.len() >= 5 {
                    break;
                }
            }
        }
        matches
    }
}
