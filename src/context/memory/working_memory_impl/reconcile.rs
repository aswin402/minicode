use super::{TaskItem, TaskItemStatus, WorkingMemory};
use std::fs;
use std::path::{Path, PathBuf};

impl WorkingMemory {
    /// Extracts potential deliverable file names or relative paths from a task title.
    /// E.g. "Create main.js — terminal preview simulation..." -> ["main.js"]
    /// E.g. "Implement user auth in src/auth.rs." -> ["src/auth.rs"]
    pub fn extract_candidate_files(title: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        for word in title.split_whitespace() {
            let mut clean = word
                .trim_matches(|c: char| {
                    !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-'
                })
                .trim_end_matches('.')
                .trim_start_matches("./");
            if clean.is_empty() {
                continue;
            }
            if clean.eq_ignore_ascii_case("dockerfile") || clean.eq_ignore_ascii_case("makefile") {
                candidates.push(clean.to_string());
                continue;
            }
            // Strip glued conjunction/action prefixes before PascalCase filenames
            // (e.g. "andWorkflowSteps.tsx" -> "WorkflowSteps.tsx", "BuildHero.tsx" -> "Hero.tsx")
            if !clean.contains('/') {
                for prefix in [
                    "and",
                    "or",
                    "Build",
                    "Create",
                    "Implement",
                    "Update",
                    "Write",
                ] {
                    if clean.len() > prefix.len() + 2 && clean.starts_with(prefix) {
                        let rest = &clean[prefix.len()..];
                        if rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                            clean = rest;
                            break;
                        }
                    }
                }
            }
            if let Some(pos) = clean.rfind('.') {
                let stem = &clean[..pos];
                let ext = &clean[pos + 1..];
                if stem.is_empty() {
                    continue;
                }
                let ext_lower = ext.to_ascii_lowercase();
                if Self::is_recognized_deliverable_extension(&ext_lower) {
                    // Bare capitalized `.js` tokens in prose (e.g. "Node.js", "Next.js", "Vue.js")
                    // are library proper nouns rather than deliverable file paths unless path-qualified.
                    let is_bare_capitalized_js = ext_lower == "js"
                        && !clean.contains('/')
                        && stem.starts_with(|c: char| c.is_ascii_uppercase());
                    if !is_bare_capitalized_js {
                        candidates.push(clean.to_string());
                    }
                }
            }
        }
        candidates
    }

    /// Returns true if `ext_lower` is a recognized source, markup, stylesheet, config, or asset extension.
    fn is_recognized_deliverable_extension(ext_lower: &str) -> bool {
        matches!(
            ext_lower,
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "py"
                | "go"
                | "c"
                | "cpp"
                | "cc"
                | "cxx"
                | "h"
                | "hpp"
                | "java"
                | "kt"
                | "kts"
                | "swift"
                | "rb"
                | "php"
                | "cs"
                | "scala"
                | "vue"
                | "svelte"
                | "astro"
                | "html"
                | "htm"
                | "css"
                | "scss"
                | "sass"
                | "less"
                | "json"
                | "toml"
                | "yaml"
                | "yml"
                | "md"
                | "mdx"
                | "sql"
                | "sh"
                | "bash"
                | "zsh"
                | "svg"
                | "xml"
                | "graphql"
                | "gql"
                | "proto"
                | "prisma"
                | "wasm"
                | "txt"
        )
    }

    /// Compares two relative file paths by their `std::path::Path` component hierarchy,
    /// returning true if they are equal or one is a component-aligned suffix of the other
    /// (e.g. `sections/Swarm.tsx` matches `src/components/sections/Swarm.tsx`).
    fn paths_match_by_components(a: &str, b: &str) -> bool {
        let clean_a = a.trim_start_matches("./");
        let clean_b = b.trim_start_matches("./");
        if clean_a.eq_ignore_ascii_case(clean_b) {
            return true;
        }
        let pa = Path::new(clean_a);
        let pb = Path::new(clean_b);
        pa.ends_with(pb) || pb.ends_with(pa)
    }

    /// Resolves a candidate file path, shorthand subpath (`sections/Swarm.tsx`), or bare filename
    /// (`Swarm.tsx`) to an existing non-empty file on disk via `Path::ends_with` component matching.
    pub(super) fn resolve_candidate_on_disk(&self, candidate: &str) -> Option<PathBuf> {
        let cand_clean = candidate.trim_start_matches("./");
        let cand_path = Path::new(cand_clean);
        let direct = self.workspace_root.join(cand_path);
        if direct.is_file() && fs::metadata(&direct).map(|m| m.len() > 0).unwrap_or(false) {
            return Some(direct);
        }
        let walker = ignore::WalkBuilder::new(&self.workspace_root)
            .max_depth(Some(5))
            .build();
        for entry in walker.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.ends_with(cand_path)
                && fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
            {
                return Some(p.to_path_buf());
            }
        }
        None
    }

    /// Returns true if `path` on disk still contains the exact unmodified content of a built-in scaffold template file.
    fn is_unmodified_scaffold_file(&self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.workspace_root) else {
            return false;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        // Only check common starter entrypoint files that exist immediately after `kit_stack_add`
        if !matches!(
            rel_str.as_str(),
            "src/App.tsx"
                | "src/App.jsx"
                | "src/main.tsx"
                | "src/main.jsx"
                | "src/index.css"
                | "index.html"
        ) {
            return false;
        }
        let Ok(disk_content) = fs::read_to_string(path) else {
            return false;
        };
        crate::tools::minikit::MiniKitScaffolder::is_scaffold_file_content(
            Some(&self.workspace_root),
            &rel_str,
            disk_content.trim(),
        )
    }

    /// Checks whether a candidate file exists on disk with non-empty, non-starter-template content.
    pub(super) fn is_candidate_completed_on_disk(&self, candidate: &str) -> bool {
        self.resolve_candidate_on_disk(candidate)
            .map(|p| !self.is_unmodified_scaffold_file(&p))
            .unwrap_or(false)
    }

    /// Splits a file path's stem on CamelCase, PascalCase, Acronym+PascalCase, snake_case, and kebab-case
    /// boundaries into lowercase tokens of length >= 3 (e.g. `CTASection.tsx` -> `["cta"]`,
    /// `CallToAction.tsx` -> `["call", "action", "cta"]`, `FeaturesBento.tsx` -> `["features", "bento"]`).
    pub(crate) fn split_identifier_stem_tokens(file_path: &str) -> Vec<String> {
        let Some(stem) = Path::new(file_path).file_stem().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let mut raw_segments: Vec<String> = Vec::new();
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut prev_lower_or_digit = false;

        for ch in stem.chars() {
            if !ch.is_ascii_alphanumeric() {
                if !current.is_empty() {
                    let seg = current.to_ascii_lowercase();
                    if seg.len() >= 3 {
                        tokens.push(seg.clone());
                    }
                    raw_segments.push(seg);
                    current.clear();
                }
                prev_lower_or_digit = false;
            } else {
                if prev_lower_or_digit && ch.is_ascii_uppercase() {
                    if !current.is_empty() {
                        let seg = current.to_ascii_lowercase();
                        if seg.len() >= 3 {
                            tokens.push(seg.clone());
                        }
                        raw_segments.push(seg);
                        current.clear();
                    }
                } else if !prev_lower_or_digit
                    && ch.is_ascii_lowercase()
                    && current.len() >= 4
                    && current.chars().all(|c| c.is_ascii_uppercase())
                {
                    let split_at = current.len() - 1;
                    let acronym = current[..split_at].to_ascii_lowercase();
                    let next_start = current[split_at..].to_string();
                    tokens.push(acronym.clone());
                    raw_segments.push(acronym);
                    current = next_start;
                }
                current.push(ch);
                prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            }
        }
        if !current.is_empty() {
            let seg = current.to_ascii_lowercase();
            if seg.len() >= 3 {
                tokens.push(seg.clone());
            }
            raw_segments.push(seg);
        }

        // Emit multi-word initialism token for >= 3 word segments (e.g., `CallToAction` -> `"cta"`)
        let filtered_segments: Vec<&String> = raw_segments
            .iter()
            .filter(|t| {
                !matches!(
                    t.as_str(),
                    "section" | "sections" | "component" | "components"
                )
            })
            .collect();
        if filtered_segments.len() >= 3
            && filtered_segments
                .iter()
                .all(|s| s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
        {
            let initialism: String = filtered_segments
                .iter()
                .filter_map(|s| s.chars().next())
                .collect();
            if initialism.len() >= 3 && !tokens.contains(&initialism) {
                tokens.push(initialism);
            }
        }

        if tokens.len() > 1 {
            let filtered: Vec<String> = tokens
                .iter()
                .filter(|t| {
                    !matches!(
                        t.as_str(),
                        "section" | "sections" | "component" | "components"
                    )
                })
                .cloned()
                .collect();
            if !filtered.is_empty() {
                return filtered;
            }
        }
        tokens
    }

    /// Reduces an English identifier word to its root stem by stripping common derivational/inflectional suffixes.
    fn morphological_root(word: &str) -> &str {
        const SUFFIXES: &[&str] = &[
            "ization", "ational", "fulness", "ousness", "iveness", "tecture", "section", "ateness",
            "ability", "ibility", "arison", "action", "ication", "ition", "ation", "ments", "ment",
            "ances", "ence", "ance", "ities", "ity", "ings", "ing", "ions", "ion", "ures", "ure",
            "ives", "ive", "ally", "ical", "als", "al", "ers", "er", "ors", "or", "ies", "es",
            "ed", "ly", "s",
        ];
        for suffix in SUFFIXES {
            if word.len() >= suffix.len() + 4 && word.ends_with(suffix) {
                return &word[..word.len() - suffix.len()];
            }
        }
        word
    }

    /// Returns whether a stem token `s` matches any alphanumeric word `w` in `task_lower`.
    fn stem_token_matches_task(s: &str, task_lower: &str) -> bool {
        let s_root = Self::morphological_root(s);
        task_lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 3)
            .any(|w| {
                if (w == "section" || w == "sections") && s != "section" && s != "sections" {
                    return false;
                }
                let w_root = Self::morphological_root(w);
                w == s
                    || (w_root.len() >= 4 && w_root == s_root)
                    || (w.len() >= 4 && s.len() >= 4 && (w.starts_with(s) || s.starts_with(w)))
                    || (w_root.len() >= 5
                        && s_root.len() >= 5
                        && (w_root.starts_with(s_root) || s_root.starts_with(w_root)))
                    || (s == "navbar" && w == "navigation")
                    || (w == "navbar" && s == "navigation")
            })
    }

    /// Extracts completed `<section id="...">`, `<header id="...">`, `<nav>`, and `<footer>`
    /// identifiers from an HTML document (requiring at least 25 lines so starter templates do not trigger).
    fn extract_html_section_ids(html_content: &str) -> std::collections::HashSet<String> {
        let mut ids = std::collections::HashSet::new();
        if html_content.lines().count() < 25 {
            return ids;
        }
        let lower = html_content.to_ascii_lowercase();
        if lower.contains("<nav") && lower.contains("</nav>") {
            ids.insert("nav".to_string());
            ids.insert("navbar".to_string());
        }
        if lower.contains("<footer") && lower.contains("</footer>") {
            ids.insert("footer".to_string());
        }

        let mut search_pos = 0;
        while let Some(rel_lt) = lower[search_pos..].find('<') {
            let tag_start = search_pos + rel_lt;
            let rest = &lower[tag_start..];
            let is_section_like = rest.starts_with("<section")
                || rest.starts_with("<header")
                || rest.starts_with("<footer")
                || rest.starts_with("<nav")
                || rest.starts_with("<main");
            if is_section_like {
                if let Some(rel_gt) = rest.find('>') {
                    let open_tag = &rest[..rel_gt];
                    // If this is a <section>, verify that a closing </section> exists after it
                    let has_closing =
                        !rest.starts_with("<section") || rest[rel_gt..].contains("</section>");
                    if has_closing {
                        for attr in ["id=\"", "id='"] {
                            if let Some(id_start_rel) = open_tag.find(attr) {
                                let quote = attr.chars().last().unwrap_or('"');
                                let val_start = id_start_rel + attr.len();
                                if let Some(val_end_rel) = open_tag[val_start..].find(quote) {
                                    let raw_id = &open_tag[val_start..val_start + val_end_rel];
                                    for tok in raw_id
                                        .split(|c: char| !c.is_ascii_alphanumeric())
                                        .filter(|t| t.len() >= 3)
                                    {
                                        ids.insert(tok.to_string());
                                    }
                                }
                            }
                        }
                    }
                    search_pos = tag_start + rel_gt + 1;
                    continue;
                }
            }
            search_pos = tag_start + 1;
        }
        ids
    }

    /// Collects identifier stem token sets for source files currently present in the workspace.
    fn collect_workspace_file_stems(&self, modified_files: &[String]) -> Vec<Vec<String>> {
        let mut stems: Vec<Vec<String>> = modified_files
            .iter()
            .map(|p| Self::split_identifier_stem_tokens(p))
            .filter(|t| !t.is_empty())
            .collect();

        let walker = ignore::WalkBuilder::new(&self.workspace_root)
            .max_depth(Some(4))
            .hidden(true)
            .build();
        for entry in walker.flatten() {
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                if let Some(path_str) = entry.path().to_str() {
                    let tokens = Self::split_identifier_stem_tokens(path_str);
                    if !tokens.is_empty() {
                        stems.push(tokens);
                    }
                }
            }
        }
        stems
    }

    /// Splits a task's primary heading into atomic deliverable clauses on conjunction separators
    /// (`" + "` or `", "`), ignoring conjunctions nested inside parenthetical remarks `(...)`,
    /// and returning each clause that contains at least one identifier token.
    pub(super) fn split_heading_clauses(primary_head: &str) -> Vec<String> {
        let mut stripped = String::with_capacity(primary_head.len());
        let mut depth = 0usize;
        for ch in primary_head.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth == 0 => stripped.push(ch),
                _ => {}
            }
        }
        let clauses: Vec<&str> = if stripped.contains(" + ") {
            stripped.split(" + ").map(str::trim).collect()
        } else if stripped.contains(", ") {
            stripped.split(", ").map(str::trim).collect()
        } else {
            vec![stripped.trim()]
        };
        clauses
            .into_iter()
            .filter(|c| {
                c.split(|ch: char| !ch.is_ascii_alphanumeric())
                    .any(|w| w.len() >= 3)
            })
            .map(String::from)
            .collect()
    }

    /// Computes the IDF-weighted alignment score `(heading_score, total_score)` between a file's
    /// identifier stem tokens and a task at `task_idx` across the full plan `tasks`.
    fn score_file_to_task(
        stem_tokens: &[String],
        task_idx: usize,
        tasks: &[TaskItem],
        task_heads: &[String],
        task_lowers: &[String],
    ) -> (f64, f64) {
        let n_tasks = tasks.len().max(1) as f64;
        let head = &task_heads[task_idx];
        let body = &task_lowers[task_idx];

        let mut head_score = 0.0;
        let mut body_score = 0.0;

        for tok in stem_tokens {
            let matches_head = Self::stem_token_matches_task(tok, head);
            let matches_body = matches_head || Self::stem_token_matches_task(tok, body);
            if !matches_body {
                continue;
            }

            let df_body = task_lowers
                .iter()
                .filter(|other_body| Self::stem_token_matches_task(tok, other_body))
                .count() as f64;
            let idf_body = (1.0 + n_tasks / (1.0 + df_body)).ln();
            body_score += idf_body;

            if matches_head {
                let df_head = task_heads
                    .iter()
                    .filter(|other_head| Self::stem_token_matches_task(tok, other_head))
                    .count() as f64;
                let idf_head = (1.0 + n_tasks / (1.0 + df_head)).ln();
                head_score += idf_head;
            }
        }

        let total_score = (2.0 * head_score) + body_score;
        (head_score, total_score)
    }

    /// Dynamically reconciles uncompleted tasks against files modified during the turn
    /// or verified on disk in the workspace using explicit deliverable verification and
    /// global IDF-weighted bipartite best-owner attribution.
    ///
    /// Returns the number of tasks updated to `Completed`.
    pub fn reconcile_workspace_tasks(&self, modified_files: &[String]) -> usize {
        let tasks = self.read_parsed_tasks();
        if tasks.is_empty() {
            return 0;
        }

        let mut html_section_ids = std::collections::HashSet::new();
        for m_path in modified_files {
            if m_path.ends_with(".html") || m_path.ends_with(".htm") {
                let full_path = self.workspace_root.join(m_path.trim_start_matches("./"));
                if let Ok(content) = fs::read_to_string(&full_path) {
                    html_section_ids.extend(Self::extract_html_section_ids(&content));
                }
            }
        }

        let task_lowers: Vec<String> = tasks.iter().map(|t| t.title.to_ascii_lowercase()).collect();
        let task_heads: Vec<String> = task_lowers
            .iter()
            .map(|lower| {
                lower
                    .split([':', '—'])
                    .next()
                    .unwrap_or(lower.as_str())
                    .trim()
                    .to_string()
            })
            .collect();

        let mut reconciled = 0;
        let mut consumed_modified: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        let mut workspace_stems_cache: Option<Vec<Vec<String>>> = None;

        for (task_idx, task) in tasks.iter().enumerate() {
            if task.status == TaskItemStatus::Completed {
                continue;
            }

            let primary_head = &task_heads[task_idx];
            let candidates = Self::extract_candidate_files(&task.title);
            let has_explicit_non_html_candidates = !candidates.is_empty()
                && !candidates
                    .iter()
                    .any(|c| c.ends_with(".html") || c.ends_with(".htm"));
            let mut file_satisfied = false;

            if candidates.len() == 1 {
                let candidate = &candidates[0];
                let matched_mod_idx = modified_files
                    .iter()
                    .position(|m| Self::paths_match_by_components(m, candidate));
                if !modified_files.is_empty() {
                    if let Some(m_idx) = matched_mod_idx {
                        file_satisfied = true;
                        consumed_modified.insert(m_idx);
                    }
                } else if task.status == TaskItemStatus::InProgress
                    && self.is_candidate_completed_on_disk(candidate)
                {
                    file_satisfied = true;
                }
            } else if candidates.len() > 1 {
                let mut all_candidates_ready = true;
                let mut any_modified_match = false;
                for candidate in &candidates {
                    let matched_mod_idx = modified_files
                        .iter()
                        .position(|m| Self::paths_match_by_components(m, candidate));
                    if let Some(m_idx) = matched_mod_idx {
                        any_modified_match = true;
                        consumed_modified.insert(m_idx);
                    } else if !self.is_candidate_completed_on_disk(candidate) {
                        all_candidates_ready = false;
                    }
                }
                if all_candidates_ready {
                    if !modified_files.is_empty() {
                        if any_modified_match {
                            file_satisfied = true;
                        }
                    } else if task.status == TaskItemStatus::InProgress {
                        file_satisfied = true;
                    }
                }
            }

            // Initial foundation task reconciliation: if the initial active step (index 0) does not name
            // a specific sub-component file and a root project manifest was created/modified, mark it satisfied.
            if !file_satisfied
                && candidates.is_empty()
                && task_idx == 0
                && task.status == TaskItemStatus::InProgress
            {
                if !modified_files.is_empty() {
                    if let Some(m_idx) = modified_files.iter().position(|m| {
                        let base = Path::new(m)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(m);
                        matches!(
                            base,
                            "package.json"
                                | "Cargo.toml"
                                | "pyproject.toml"
                                | "go.mod"
                                | "minikit.json"
                                | "index.html"
                        )
                    }) {
                        file_satisfied = true;
                        consumed_modified.insert(m_idx);
                    }
                } else if crate::agent::orchestrator::WorkflowRouter::project_manifest(
                    &self.workspace_root,
                )
                .is_some()
                {
                    file_satisfied = true;
                }
            }

            // In-file HTML `<section id="...">` / `<nav>` / `<footer>` DOM ID reconciliation:
            // Only applies when an unconsumed HTML file was modified and the task does not name a non-HTML file.
            let has_unconsumed_html = modified_files.iter().enumerate().any(|(idx, m)| {
                !consumed_modified.contains(&idx) && (m.ends_with(".html") || m.ends_with(".htm"))
            });
            if !file_satisfied
                && has_unconsumed_html
                && !has_explicit_non_html_candidates
                && !html_section_ids.is_empty()
            {
                let clauses = Self::split_heading_clauses(primary_head);
                let all_clauses_in_dom = !clauses.is_empty()
                    && clauses.iter().all(|clause| {
                        html_section_ids
                            .iter()
                            .any(|sec_id| Self::stem_token_matches_task(sec_id, clause))
                    });
                if all_clauses_in_dom {
                    file_satisfied = true;
                }
            }

            // Global IDF-Weighted Bipartite Best-Owner Attribution:
            // Attribute each unconsumed modified file `m_path` to task `task_idx` only if:
            // 1. `task_idx` matches the file in its primary deliverable heading (`head_score > 0.0`).
            // 2. `task_idx` achieves the global maximum alignment score across ALL tasks in the plan
            //    (including already-Completed tasks, preventing re-edits of an earlier task's file
            //    from spilling over to a later task).
            // 3. Every deliverable sub-clause of `primary_head` is covered by workspace files.
            if !file_satisfied && candidates.is_empty() && !modified_files.is_empty() {
                for (m_idx, m_path) in modified_files.iter().enumerate() {
                    if consumed_modified.contains(&m_idx) {
                        continue;
                    }
                    let stem_tokens = Self::split_identifier_stem_tokens(m_path);
                    if stem_tokens.is_empty() {
                        continue;
                    }

                    let (head_score, total_score) = Self::score_file_to_task(
                        &stem_tokens,
                        task_idx,
                        &tasks,
                        &task_heads,
                        &task_lowers,
                    );
                    if head_score <= 0.0 {
                        continue;
                    }

                    // Global best-owner check across all tasks (including Completed tasks)
                    let mut best_global_score = 0.0_f64;
                    let mut best_completed_score = 0.0_f64;
                    let mut tied_uncompleted_count = 0usize;

                    for other_idx in 0..tasks.len() {
                        let (_, other_total) = Self::score_file_to_task(
                            &stem_tokens,
                            other_idx,
                            &tasks,
                            &task_heads,
                            &task_lowers,
                        );
                        if other_total > best_global_score {
                            best_global_score = other_total;
                        }
                        if tasks[other_idx].status == TaskItemStatus::Completed {
                            if other_total > best_completed_score {
                                best_completed_score = other_total;
                            }
                        } else if (other_total - total_score).abs() < 1e-6 {
                            tied_uncompleted_count += 1;
                        }
                    }

                    // If an already-Completed task owns this file with >= score, consume the file and skip
                    if best_completed_score >= total_score - 1e-6 {
                        consumed_modified.insert(m_idx);
                        continue;
                    }

                    // Must achieve the global maximum score across the entire plan
                    if total_score + 1e-6 < best_global_score {
                        continue;
                    }

                    // If multiple uncompleted tasks tie for the exact same score, only match the active InProgress task
                    if tied_uncompleted_count > 1 && task.status != TaskItemStatus::InProgress {
                        continue;
                    }

                    // Sub-clause coverage check: if the task's primary heading lists multiple deliverable clauses
                    // (`A + B + C` or `A, B, C`), verify that all clauses are covered by workspace files.
                    let clauses = Self::split_heading_clauses(primary_head);
                    if clauses.len() > 1 {
                        let ws_stems = workspace_stems_cache.get_or_insert_with(|| {
                            self.collect_workspace_file_stems(modified_files)
                        });
                        let all_clauses_covered = clauses.iter().all(|clause| {
                            ws_stems.iter().any(|file_stems| {
                                file_stems
                                    .iter()
                                    .any(|tok| Self::stem_token_matches_task(tok, clause))
                            })
                        });
                        if !all_clauses_covered {
                            consumed_modified.insert(m_idx);
                            continue;
                        }
                    }

                    file_satisfied = true;
                    consumed_modified.insert(m_idx);
                    break;
                }
            }

            if file_satisfied {
                if let Ok(()) = self.update_progress(&task.title, "completed") {
                    reconciled += 1;
                }
            }
        }

        let final_tasks = self.read_parsed_tasks();
        if !final_tasks.is_empty()
            && final_tasks
                .iter()
                .all(|t| t.status == TaskItemStatus::Completed)
        {
            let p_path = self.progress_path();
            if let Ok(content) = fs::read_to_string(&p_path) {
                if content.contains("- Status: In Progress") {
                    let updated =
                        content.replacen("- Status: In Progress", "- Status: Completed (100%)", 1);
                    let _ = fs::write(&p_path, updated);
                }
            }
        }

        reconciled
    }

    /// Reconciles any remaining deliverable plan tasks whose files are already present on disk
    /// when `power_verify` runs.
    pub fn reconcile_verification_tasks(&self) -> usize {
        self.reconcile_workspace_tasks(&[])
    }

    /// Formats a concise status hint showing how many plan tasks are `[x] Completed` and listing
    /// ONLY the remaining uncompleted task numbers/titles (if any) so the agent never wastes turns
    /// calling `update_progress` on steps that were already auto-reconciled to `[x]`.
    pub fn format_remaining_tasks_hint(&self) -> String {
        let tasks = self.read_parsed_tasks();
        if tasks.is_empty() {
            return String::new();
        }
        let total = tasks.len();
        let remaining: Vec<(usize, &TaskItem)> = tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status != TaskItemStatus::Completed)
            .collect();
        let completed = total.saturating_sub(remaining.len());

        if remaining.is_empty() {
            format!(
                "📋 Plan Status: All {}/{} tasks are already marked `[x]` Completed. Do NOT call `update_progress` — call `archive_plan` directly when ready to finalize.",
                total, total
            )
        } else {
            let rem_list = remaining
                .iter()
                .take(5)
                .map(|(idx, t)| format!("#{} \"{}\"", idx + 1, t.title))
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if remaining.len() > 5 {
                format!(" (+{} more)", remaining.len() - 5)
            } else {
                String::new()
            };
            let done_clause = if completed == 0 {
                String::new()
            } else {
                let done_ids = tasks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.status == TaskItemStatus::Completed)
                    .map(|(idx, _)| format!("#{}", idx + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(" (already `[x]`: {} — do not re-update)", done_ids)
            };
            format!(
                "📋 Plan Status: {}/{} tasks `[x]` Completed{}. Remaining uncompleted step(s): {}{}",
                completed, total, done_clause, rem_list, suffix
            )
        }
    }
}
