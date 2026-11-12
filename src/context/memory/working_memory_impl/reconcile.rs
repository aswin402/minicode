use super::{TaskItem, TaskItemStatus, VerificationEvidenceState, WorkingMemory};
use std::fs;
use std::path::{Path, PathBuf};

/// Readiness state of the currently active `[>]` step after a workspace mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveStepReadiness {
    /// Step still has unfinished file deliverables or has no auto-detectable file deliverables.
    InProgress,
    /// All declared file deliverables for the active step exist on disk, and the step does not
    /// require runtime build/lint/browser verification — ready for immediate `update_progress`.
    DeliverablesReady,
    /// Declared file deliverables for the active step exist on disk, AND the step also requires
    /// runtime build/lint/browser verification before calling `update_progress`.
    AwaitingVerification,
}

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

    /// Returns true if `path` ends with `suffix` on `Component::Normal` boundaries using
    /// ASCII case-insensitive comparison (`src/components/sections/Cta.tsx` matches `CTA.tsx`).
    fn path_ends_with_ignore_ascii_case(path: &Path, suffix: &Path) -> bool {
        let path_comps: Vec<&str> = path
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        let suffix_comps: Vec<&str> = suffix
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        if suffix_comps.is_empty() || path_comps.len() < suffix_comps.len() {
            return false;
        }
        let offset = path_comps.len() - suffix_comps.len();
        path_comps[offset..]
            .iter()
            .zip(suffix_comps.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }

    /// Compares two relative file paths by their `std::path::Path` component hierarchy,
    /// returning true if they are equal or one is a component-aligned suffix of the other
    /// (e.g. `sections/Swarm.tsx` matches `src/components/sections/Swarm.tsx`, `CTA.tsx` matches `Cta.tsx`).
    fn paths_match_by_components(a: &str, b: &str) -> bool {
        let clean_a = a.trim_start_matches("./");
        let clean_b = b.trim_start_matches("./");
        if clean_a.eq_ignore_ascii_case(clean_b) {
            return true;
        }
        let pa = Path::new(clean_a);
        let pb = Path::new(clean_b);
        Self::path_ends_with_ignore_ascii_case(pa, pb)
            || Self::path_ends_with_ignore_ascii_case(pb, pa)
    }

    /// Resolves a candidate file path, shorthand subpath (`sections/Swarm.tsx`), or bare filename
    /// (`Swarm.tsx` / `CTA.tsx`) to an existing non-empty file on disk via component suffix matching.
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
                && Self::path_ends_with_ignore_ascii_case(p, cand_path)
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
        let base = Path::new(&rel_str)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(rel_str.as_str());
        // Root project manifests (`package.json`, `Cargo.toml`, etc.) indicate scaffolding succeeded
        // when referenced by foundation steps; starter source/template files under `src/`, `app/`,
        // `public/`, or `index.html` must be modified before counting as completed deliverables.
        if matches!(
            base,
            "package.json"
                | "Cargo.toml"
                | "pyproject.toml"
                | "go.mod"
                | "minikit.json"
                | "tsconfig.json"
                | "tsconfig.app.json"
                | "tsconfig.node.json"
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
            .filter(|t| !Self::is_structural_stem(t))
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

        tokens.retain(|t| !Self::is_structural_stem(t));

        // Also treat any trailing/wrapping segment that merely repeats an ancestor directory name
        // (e.g. `src/widgets/CodeWidget.tsx` -> `widgets`) as structural when specific stem tokens remain.
        if tokens.len() > 1 {
            let dir_roots: Vec<String> = Path::new(file_path)
                .parent()
                .into_iter()
                .flat_map(|p| p.components())
                .filter_map(|c| c.as_os_str().to_str())
                .map(|seg| seg.to_ascii_lowercase())
                .filter(|seg| seg.len() >= 4)
                .collect();
            if !dir_roots.is_empty() {
                let filtered: Vec<String> = tokens
                    .iter()
                    .filter(|t| {
                        !dir_roots.iter().any(|d| {
                            d == t.as_str()
                                || d.strip_suffix('s') == Some(t.as_str())
                                || t.strip_suffix('s') == Some(d.as_str())
                        })
                    })
                    .cloned()
                    .collect();
                if !filtered.is_empty() {
                    tokens = filtered;
                }
            }
        }

        tokens
    }

    #[inline]
    fn is_structural_stem(word: &str) -> bool {
        crate::constants::STRUCTURAL_CONTAINER_STEMS.contains(&word)
    }

    /// Reduces an English identifier word to its root stem by stripping common derivational/inflectional suffixes.
    fn morphological_root(word: &str) -> &str {
        const SUFFIXES: &[&str] = &[
            "ization", "ational", "fulness", "ousness", "iveness", "tecture", "section", "ateness",
            "ability", "ibility", "action", "ication", "ition", "ation", "ments", "ment", "ances",
            "ence", "ance", "ities", "ison", "ity", "ings", "ing", "ions", "ion", "ures", "ure",
            "ives", "ive", "ally", "ical", "als", "al", "ers", "er", "ors", "or", "ies", "es",
            "ed", "ly", "s",
        ];
        for suffix in SUFFIXES {
            if word.len() >= suffix.len() + 4 && word.ends_with(suffix) {
                return &word[..word.len() - suffix.len()];
            }
        }
        if word.len() >= 6 && word.ends_with('e') {
            return &word[..word.len() - 1];
        }
        word
    }

    /// Returns whether a stem token `s` matches any alphanumeric word `w` in `task_lower`.
    fn stem_token_matches_task(s: &str, task_lower: &str) -> bool {
        if Self::is_structural_stem(s) {
            return false;
        }
        let s_root = Self::morphological_root(s);
        task_lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 3)
            .any(|w| {
                if Self::is_structural_stem(w) {
                    return false;
                }
                let w_root = Self::morphological_root(w);
                w == s
                    || (w_root.len() >= 4 && w_root == s_root)
                    || (w.len() >= 4 && s.len() >= 4 && (w.starts_with(s) || s.starts_with(w)))
                    || (w_root.len() >= 5
                        && s_root.len() >= 5
                        && (w_root.starts_with(s_root) || s_root.starts_with(w_root)))
                    || (matches!(s, "nav" | "navbar" | "navigation")
                        && matches!(w, "nav" | "navbar" | "navigation"))
            })
    }

    /// Checks whether a multi-segment identifier `file_stems` has sufficient segment alignment
    /// to complete a single-clause task `clause` / `task_lower` on its own.
    /// When both the file stem has `>= 2` segments and the clause has `>= 2` non-structural words,
    /// requires at least 2 segments of the file to match `task_lower` so a 2-segment helper whose
    /// second segment diverges from the clause (e.g. `CodeBlock.tsx` vs `"Code Intelligence"`)
    /// does not prematurely complete the task on a single-word overlap.
    fn single_clause_segments_align(file_stems: &[String], clause: &str, task_lower: &str) -> bool {
        let clause_word_count = clause
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 3 && !Self::is_structural_stem(w))
            .count();
        if file_stems.len() >= 2 && clause_word_count >= 2 {
            let matched_segments = file_stems
                .iter()
                .filter(|tok| Self::stem_token_matches_task(tok, task_lower))
                .count();
            return matched_segments >= 2;
        }
        file_stems
            .iter()
            .any(|tok| Self::stem_token_matches_task(tok, clause))
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

    /// Collects `(workspace_relative_path, stem_tokens)` for non-empty, non-scaffold source files
    /// present in `modified_files` or discovered in the workspace (up to depth 6), excluding
    /// internal `.minicode/` and `minikit_docs/` specification files.
    fn collect_workspace_files_with_stems(
        &self,
        modified_files: &[String],
    ) -> Vec<(String, Vec<String>)> {
        let mut results: Vec<(String, Vec<String>)> = Vec::new();
        let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

        for p in modified_files {
            let normalized = Path::new(p)
                .strip_prefix(&self.workspace_root)
                .map(|rel| rel.to_string_lossy().into_owned())
                .unwrap_or_else(|_| p.trim_start_matches("./").to_string())
                .replace('\\', "/");
            if normalized.starts_with(".minicode/") || normalized.starts_with("minikit_docs/") {
                continue;
            }
            let tokens = Self::split_identifier_stem_tokens(&normalized);
            if !tokens.is_empty() && seen_paths.insert(normalized.to_ascii_lowercase()) {
                results.push((normalized, tokens));
            }
        }

        let walker = ignore::WalkBuilder::new(&self.workspace_root)
            .max_depth(Some(6))
            .hidden(true)
            .build();
        for entry in walker.flatten() {
            let path = entry.path();
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            if !fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
                continue;
            }
            if self.is_unmodified_scaffold_file(path) {
                continue;
            }
            let Ok(rel) = path.strip_prefix(&self.workspace_root) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.starts_with(".minicode/") || rel_str.starts_with("minikit_docs/") {
                continue;
            }
            let tokens = Self::split_identifier_stem_tokens(&rel_str);
            if !tokens.is_empty() && seen_paths.insert(rel_str.to_ascii_lowercase()) {
                results.push((rel_str, tokens));
            }
        }
        results
    }

    /// Returns true if `word` (or its morphological root) denotes a verification/QA gate phase.
    #[inline]
    fn is_verification_phase_word(word: &str) -> bool {
        let lower = word.to_ascii_lowercase();
        let root = Self::morphological_root(&lower);
        crate::constants::VERIFICATION_PHASE_TOKENS
            .iter()
            .any(|&v| v == lower || Self::morphological_root(v) == root)
    }

    /// Splits CamelCase, PascalCase, and Acronym+PascalCase identifier compounds into
    /// space-separated lowercase words while preserving punctuation and parenthetical grouping
    /// (e.g. `CodeIntelligence` -> `code intelligence`, `CTASection` -> `cta section`,
    /// `FinalCta` -> `final cta`, while keeping pure acronyms like `NDJSON` / `TUI` intact).
    fn expand_camelcase_to_lower(text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len() + 8);
        let mut prev_lower_or_digit = false;
        let mut prev_upper = false;
        for (idx, &ch) in chars.iter().enumerate() {
            let next_is_lower = chars.get(idx + 1).is_some_and(|c| c.is_ascii_lowercase());
            if ch.is_ascii_uppercase() && (prev_lower_or_digit || (prev_upper && next_is_lower)) {
                out.push(' ');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            prev_upper = ch.is_ascii_uppercase();
        }
        out
    }

    /// Returns true if `clause` begins with an operational setup, discovery, scaffolding, or
    /// verification verb rather than naming a concrete code/UI deliverable artifact.
    fn clause_starts_with_operational_verb(clause: &str) -> bool {
        let Some(first) = clause
            .split(|c: char| !c.is_ascii_alphanumeric())
            .find(|w| w.len() >= 2)
        else {
            return true;
        };
        Self::is_verification_phase_word(first)
            || matches!(
                first,
                "scaffold"
                    | "init"
                    | "initialize"
                    | "bootstrap"
                    | "install"
                    | "setup"
                    | "read"
                    | "inspect"
                    | "explore"
                    | "analyze"
                    | "review"
                    | "run"
                    | "start"
                    | "launch"
                    | "stop"
                    | "kill"
                    | "deploy"
                    | "commit"
                    | "push"
                    | "archive"
                    | "calibrate"
            )
    }

    /// Returns true when `expanded_head` enumerates `>= 2` deliverable artifact clauses
    /// (e.g. `"nav + hero + live terminal"`, `"security + browser automation sections"`,
    /// `"tools + interfaces + comparison + cta"`) rather than setup/scaffolding/verification actions.
    fn is_multi_clause_deliverable_heading(raw_title: &str, expanded_head: &str) -> bool {
        if Self::is_verification_gate_step(raw_title)
            || raw_title.to_ascii_lowercase().contains("kit_stack_add")
        {
            return false;
        }
        let clauses = Self::split_heading_clauses(expanded_head);
        clauses.len() >= 2
            && clauses
                .iter()
                .all(|c| !Self::clause_starts_with_operational_verb(c))
    }

    /// Parses the primary deliverable clause string from a mixed-case task title along with
    /// whether the title explicitly enumerates a multi-deliverable group:
    /// - Strips trailing descriptive clauses after `—` (em-dash) or `;`.
    /// - When structured as `<Category Label>: <Clause 1>, <Clause 2>, ...` where `<Category Label>`
    ///   is a non-verification category noun phrase (`>= 2` words or plural category noun) and the RHS
    ///   enumerates `>= 2` capitalized deliverable items outside parentheses, promotes the RHS clauses.
    /// - Otherwise, when the LHS (or un-coloned heading) itself enumerates `>= 2` deliverable noun-phrase
    ///   clauses joined by `" + "` or `", "`, marks it as a deliverable group as well.
    fn parse_deliverable_head(raw_title: &str) -> (String, bool) {
        let before_dash = raw_title
            .split('—')
            .next()
            .unwrap_or(raw_title)
            .split(';')
            .next()
            .unwrap_or(raw_title)
            .trim();

        if let Some((lhs, rhs)) = before_dash.split_once(':') {
            let lhs_words: Vec<&str> = lhs
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| w.len() >= 2)
                .collect();
            let lhs_is_deliverable_category = (lhs_words.len() >= 2
                || lhs_words
                    .first()
                    .is_some_and(|w| w.ends_with('s') || w.ends_with('S')))
                && !lhs_words
                    .iter()
                    .any(|w| Self::is_verification_phase_word(w));

            let mut rhs_no_parens = String::with_capacity(rhs.len());
            let mut depth = 0usize;
            for ch in rhs.chars() {
                match ch {
                    '(' => depth += 1,
                    ')' => depth = depth.saturating_sub(1),
                    _ if depth == 0 => rhs_no_parens.push(ch),
                    _ => {}
                }
            }
            let rhs_clauses: Vec<&str> = if rhs_no_parens.contains(" + ") {
                rhs_no_parens.split(" + ").map(str::trim).collect()
            } else if rhs_no_parens.contains(", ") {
                rhs_no_parens.split(", ").map(str::trim).collect()
            } else {
                Vec::new()
            };
            let is_capitalized_deliverable_list = lhs_is_deliverable_category
                && rhs_clauses.len() >= 2
                && rhs_clauses.iter().all(|c| {
                    c.chars()
                        .find(|ch| ch.is_ascii_alphanumeric())
                        .is_some_and(|ch| ch.is_ascii_uppercase())
                });
            if is_capitalized_deliverable_list {
                return (Self::expand_camelcase_to_lower(rhs.trim()), true);
            }
            let expanded_lhs = Self::expand_camelcase_to_lower(lhs.trim());
            let is_group = Self::is_multi_clause_deliverable_heading(raw_title, &expanded_lhs);
            return (expanded_lhs, is_group);
        }

        let expanded = Self::expand_camelcase_to_lower(before_dash);
        let is_group = Self::is_multi_clause_deliverable_heading(raw_title, &expanded);
        (expanded, is_group)
    }

    /// Extracts the primary deliverable clause string from a mixed-case task title.
    fn extract_primary_deliverable_head(raw_title: &str) -> String {
        Self::parse_deliverable_head(raw_title).0
    }

    /// Returns true if `token` exhibits compound code-identifier morphology (`CamelCase`, `PascalCase`,
    /// `AcronymPascal`, or `snake_case`) rather than plain single-case English prose.
    fn has_compound_identifier_morphology(token: &str) -> bool {
        let clean = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        if clean.len() < 4 {
            return false;
        }
        if clean.contains('_') && clean.split('_').filter(|seg| !seg.is_empty()).count() >= 2 {
            return true;
        }
        let chars: Vec<char> = clean.chars().collect();
        let mut seen_lower = false;
        for (i, &ch) in chars.iter().enumerate() {
            if ch.is_ascii_lowercase() {
                seen_lower = true;
            } else if ch.is_ascii_uppercase() && i > 0 {
                let next_lower = chars.get(i + 1).is_some_and(|c| c.is_ascii_lowercase());
                if seen_lower || (i >= 2 && next_lower) {
                    return true;
                }
            }
        }
        false
    }

    /// Returns true if `raw_title` is a command/browser/QA verification gate without explicit file deliverables.
    fn is_verification_gate_step(raw_title: &str) -> bool {
        if !Self::extract_candidate_files(raw_title).is_empty() {
            return false;
        }
        let first_word = raw_title
            .split(|c: char| !c.is_ascii_alphanumeric())
            .find(|w| w.len() >= 2)
            .unwrap_or("");
        Self::is_verification_phase_word(first_word)
            || first_word.eq_ignore_ascii_case("test")
            || first_word.eq_ignore_ascii_case("run")
            || first_word.eq_ignore_ascii_case("check")
    }

    /// Returns true when a step requires runtime command, build, lint, test, or browser verification
    /// (either a pure verification gate step OR a compound deliverable + verification step such as
    /// `"Compose page, verify build and responsive geometry: Compose all sections in src/pages/HomePage.tsx; run bun run build..."`).
    /// Strips parenthetical remarks `(...)` first so UI copy or diagram labels inside parentheses
    /// (e.g. `(DAG with goal -> verify -> merge)`) never trigger false positives.
    pub(crate) fn step_requires_runtime_verification(raw_title: &str) -> bool {
        if Self::is_verification_gate_step(raw_title) {
            return true;
        }
        let mut outside_parens = String::with_capacity(raw_title.len());
        let mut depth = 0usize;
        for ch in raw_title.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth == 0 => outside_parens.push(ch),
                _ => {}
            }
        }
        let lower_unparenthesized = outside_parens.to_ascii_lowercase();
        for tool_cue in [
            "browser_screenshot",
            "browser_qa_audit",
            "browser_evaluate",
            "power_verify",
            "power_review",
        ] {
            if lower_unparenthesized.contains(tool_cue) {
                return true;
            }
        }

        for major_seg in lower_unparenthesized.split([';', '—', ':', ',']) {
            for sub_clause in major_seg.split(" and ") {
                let words: Vec<&str> = sub_clause
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .filter(|w| w.len() >= 2)
                    .collect();
                let Some(&first) = words.first() else {
                    continue;
                };
                if matches!(
                    first,
                    "verify" | "validate" | "audit" | "typecheck" | "lint" | "probe"
                ) {
                    return true;
                }
                if first == "run"
                    && words.iter().skip(1).any(|&w| {
                        matches!(
                            w,
                            "build"
                                | "lint"
                                | "test"
                                | "tests"
                                | "typecheck"
                                | "check"
                                | "clippy"
                                | "tsc"
                                | "pytest"
                                | "vitest"
                        )
                    })
                {
                    return true;
                }
                if matches!(first, "start" | "launch") && words.get(1) == Some(&"dev") {
                    return true;
                }
            }
        }
        false
    }

    /// Strips parenthetical remarks `(...)` and lowercases `raw_title` for structural step inspection.
    fn strip_parentheticals_lower(raw_title: &str) -> String {
        let mut outside_parens = String::with_capacity(raw_title.len());
        let mut depth = 0usize;
        for ch in raw_title.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth == 0 => outside_parens.push(ch.to_ascii_lowercase()),
                _ => {}
            }
        }
        outside_parens
    }

    /// Returns true when a step requires static CLI compilation, typechecking, linting, or unit tests.
    pub fn step_requires_static_build_or_lint(raw_title: &str) -> bool {
        let lower = Self::strip_parentheticals_lower(raw_title);
        lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|w| {
                matches!(
                    w,
                    "build"
                        | "lint"
                        | "typecheck"
                        | "tsc"
                        | "clippy"
                        | "cargo"
                        | "pytest"
                        | "vitest"
                        | "jest"
                        | "power_verify"
                )
            })
    }

    /// Returns true when a step requires live dev-server / browser / DOM / screenshot verification.
    pub fn step_requires_browser_verification(raw_title: &str) -> bool {
        let lower = Self::strip_parentheticals_lower(raw_title);
        if lower.contains("browser_") || lower.contains("dev server") || lower.contains("localhost")
        {
            return true;
        }
        lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
            matches!(
                w,
                "browser"
                    | "screenshot"
                    | "screenshots"
                    | "viewport"
                    | "viewports"
                    | "playwright"
                    | "responsive"
            )
        })
    }

    /// Returns true when a step explicitly calls for multi-viewport, tablet, or mobile browser QA.
    pub fn step_mentions_multi_viewport_qa(raw_title: &str) -> bool {
        let lower = Self::strip_parentheticals_lower(raw_title);
        if lower.contains("3-viewport") || lower.contains("multi-viewport") {
            return true;
        }
        lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
            matches!(
                w,
                "mobile"
                    | "tablet"
                    | "iphone"
                    | "ipad"
                    | "viewports"
                    | "390"
                    | "375"
                    | "768"
                    | "390x844"
                    | "768x1024"
            )
        })
    }

    /// Automatically decomposes a monolithic verification macro step (one that declares no new
    /// file deliverables and bundles BOTH static CLI build/lint checks AND live browser/viewport QA)
    /// into crisp Tier-2 tactical sub-steps so progress advances stage-by-stage instead of freezing
    /// on a single final step until `archive_plan`.
    pub fn synthesize_compound_verification_substeps(raw_title: &str) -> Vec<String> {
        if Self::task_declares_verifiable_deliverables(raw_title)
            || !Self::step_requires_static_build_or_lint(raw_title)
            || !Self::step_requires_browser_verification(raw_title)
        {
            return Vec::new();
        }
        if Self::step_mentions_multi_viewport_qa(raw_title) {
            vec![
                "Run static build, typecheck, and lint verification".to_string(),
                "Start dev server and verify desktop browser layout & QA".to_string(),
                "Verify responsive tablet & mobile viewports and resolve layout issues".to_string(),
            ]
        } else {
            vec![
                "Run static build, typecheck, and lint verification".to_string(),
                "Start dev server and verify live browser layout & QA".to_string(),
            ]
        }
    }

    /// Returns true if `tool_name` + `tool_args` switches the browser to a tablet or mobile viewport
    /// (`width <= 900` or preset `mobile`/`mobile_large`/`iphone`/`tablet`/`ipad`), either directly
    /// via `browser_emulate` or inside a `browser_batch` step.
    fn tool_switches_to_responsive_viewport(
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> bool {
        let check_emulate_obj = |obj: &serde_json::Value| -> bool {
            if let Some(vp) = obj
                .get("viewport")
                .or_else(|| obj.get("preset"))
                .or_else(|| obj.get("device"))
                .and_then(|v| v.as_str())
            {
                let lower = vp.trim().to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "mobile" | "mobile_large" | "iphone" | "tablet" | "ipad"
                ) {
                    return true;
                }
                if let Some((w_str, _)) = lower.split_once('x') {
                    if w_str.trim().parse::<u32>().is_ok_and(|w| w > 0 && w <= 900) {
                        return true;
                    }
                }
            }
            if let Some(w) = obj.get("width").and_then(|v| {
                v.as_u64().or_else(|| {
                    v.as_str()
                        .and_then(|s| s.trim().trim_end_matches("px").trim().parse::<u64>().ok())
                })
            }) {
                if w > 0 && w <= 900 {
                    return true;
                }
            }
            false
        };

        match tool_name {
            "browser_emulate" => check_emulate_obj(tool_args),
            "browser_batch" => tool_args
                .get("steps")
                .or_else(|| tool_args.get("actions"))
                .and_then(|v| v.as_array())
                .is_some_and(|arr| {
                    arr.iter().any(|item| {
                        item.get("action")
                            .and_then(|a| a.as_str())
                            .is_some_and(|act| {
                                matches!(
                                    act,
                                    "emulate" | "emulate_viewport" | "set_viewport" | "resize"
                                )
                            })
                            && check_emulate_obj(item)
                    })
                }),
            _ => false,
        }
    }

    /// Returns true if `tool_name` + `tool_args` executes an interactive/visual browser QA inspection
    /// (screenshot, layout QA audit, DOM eval, or batch containing one of those actions).
    pub(crate) fn tool_performs_browser_qa_check(
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> bool {
        match tool_name {
            "browser_screenshot" | "browser_qa_audit" | "browser_eval" => true,
            "browser_batch" => tool_args
                .get("steps")
                .or_else(|| tool_args.get("actions"))
                .and_then(|v| v.as_array())
                .is_some_and(|arr| {
                    arr.iter().any(|item| {
                        item.get("action")
                            .and_then(|a| a.as_str())
                            .is_some_and(|act| {
                                matches!(
                                    act,
                                    "screenshot" | "qa_audit" | "audit" | "eval" | "evaluate"
                                )
                            })
                    })
                }),
            _ => false,
        }
    }

    /// Returns true when `title`'s required runtime/browser verification stage has already been
    /// observed in `ev` (or when `title` does not require runtime/browser verification).
    pub(crate) fn is_verification_step_evidence_satisfied(
        &self,
        title: &str,
        ev: &VerificationEvidenceState,
    ) -> bool {
        if Self::step_mentions_multi_viewport_qa(title) {
            ev.browser_responsive_verified
        } else if Self::step_requires_browser_verification(title) {
            ev.browser_desktop_verified || ev.browser_responsive_verified
        } else if Self::step_requires_static_build_or_lint(title) {
            ev.static_build_verified
        } else {
            true
        }
    }

    /// Records the currently active `[>]` step as having been auto-promoted by background
    /// reconciliation so an immediate `update_progress(step="active", status="completed")` call
    /// intended for the just-closed previous step cannot prematurely close the newly promoted step
    /// before its own deliverables or verification checks have run.
    pub fn record_auto_promoted_active_step(&self) {
        if let Some(active) = self
            .read_parsed_tasks()
            .into_iter()
            .find(|t| t.status == TaskItemStatus::InProgress)
        {
            let mut ev = self.load_verification_evidence();
            if ev.auto_promoted_step.as_deref() != Some(active.title.as_str()) {
                ev.auto_promoted_step = Some(active.title);
                self.save_verification_evidence(&ev);
            }
        }
    }

    /// Records runtime/browser verification tool execution against `.minicode/plan/verification_evidence.json`.
    pub fn record_verification_tool_activity(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_success: bool,
    ) {
        if !tool_success || !self.has_active_plan() {
            return;
        }
        let mut ev = self.load_verification_evidence();
        let mut changed = false;

        if Self::tool_switches_to_responsive_viewport(tool_name, tool_args) {
            if !ev.responsive_mode_active {
                ev.responsive_mode_active = true;
                changed = true;
            }
        } else if tool_name == "browser_emulate" {
            let switches_to_desktop = tool_args
                .get("width")
                .and_then(|w| w.as_u64())
                .is_some_and(|w| w > 900)
                || tool_args
                    .get("viewport")
                    .or_else(|| tool_args.get("preset"))
                    .or_else(|| tool_args.get("device"))
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.to_ascii_lowercase().contains("desktop"));
            if switches_to_desktop && ev.responsive_mode_active {
                ev.responsive_mode_active = false;
                changed = true;
            }
        }

        if Self::tool_performs_browser_qa_check(tool_name, tool_args) {
            if ev.responsive_mode_active {
                if !ev.browser_responsive_verified {
                    ev.browser_responsive_verified = true;
                    changed = true;
                }
            } else if !ev.browser_desktop_verified {
                ev.browser_desktop_verified = true;
                changed = true;
            }
        }

        let is_static_verify = tool_name == "power_verify"
            || (tool_name == "exec_cmd"
                && tool_args
                    .get("command")
                    .or_else(|| tool_args.get("cmd"))
                    .and_then(|v| v.as_str())
                    .is_some_and(Self::step_requires_static_build_or_lint));
        if is_static_verify && !ev.static_build_verified {
            ev.static_build_verified = true;
            changed = true;
        }

        if let Some(promoted) = ev.auto_promoted_step.clone() {
            if self.is_verification_step_evidence_satisfied(&promoted, &ev)
                && (!Self::task_declares_verifiable_deliverables(&promoted)
                    || self.active_step_readiness() != ActiveStepReadiness::InProgress)
            {
                ev.auto_promoted_step = None;
                changed = true;
            }
        }

        if changed {
            self.save_verification_evidence(&ev);
        }
    }

    /// Returns true if `tool_name` / `tool_args` runs a static build, typecheck, lint, or verification gate.
    fn tool_runs_static_build_or_lint(tool_name: &str, tool_args: &serde_json::Value) -> bool {
        if tool_name == "power_verify" {
            return true;
        }
        if !matches!(
            tool_name,
            "exec_cmd" | "sandbox_exec" | "exec" | "bash" | "run_command" | "minitask"
        ) {
            return false;
        }
        if tool_name == "minitask"
            && !tool_args
                .get("action")
                .and_then(|a| a.as_str())
                .is_some_and(|act| {
                    act.eq_ignore_ascii_case("run") || act.eq_ignore_ascii_case("exec")
                })
        {
            return false;
        }
        let Some(cmd) = tool_args
            .get("command")
            .or_else(|| tool_args.get("cmd"))
            .or_else(|| tool_args.get("target"))
            .and_then(|v| v.as_str())
        else {
            return false;
        };
        let lower = cmd.to_ascii_lowercase();
        [
            "bun run build",
            "npm run build",
            "pnpm build",
            "pnpm run build",
            "yarn build",
            "bun run lint",
            "npm run lint",
            "tsc",
            "cargo check",
            "cargo build",
            "cargo test",
            "cargo clippy",
            "pytest",
            "vitest",
        ]
        .iter()
        .any(|cue| lower.contains(cue))
    }

    /// Advances an active deliverable step into a trailing verification gate step (or advances an
    /// active Tier-2 verification substep across static -> desktop browser -> responsive QA stages)
    /// when tool execution proves the agent has transitioned stages without calling `update_progress`.
    pub fn reconcile_verification_substep_on_tool_transition(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_success: bool,
    ) -> usize {
        if !tool_success {
            return 0;
        }
        let mut reconciled = 0usize;

        let starts_browser_stage = match tool_name {
            "minitask" => tool_args
                .get("action")
                .and_then(|a| a.as_str())
                .is_some_and(|act| {
                    act.eq_ignore_ascii_case("start") || act.eq_ignore_ascii_case("restart")
                }),
            "browser_navigate" | "browser_qa_audit" | "browser_screenshot" | "browser_emulate"
            | "browser_batch" => true,
            _ => false,
        };

        // Direct macro transition: if the active step is a completed pure-deliverable step and the
        // immediate next step is a verification gate, starting verification tools auto-completes the
        // deliverable step so it never stays stuck until the end of verification.
        if !self.is_tactical_subplan()
            && (starts_browser_stage || Self::tool_runs_static_build_or_lint(tool_name, tool_args))
        {
            let tasks = self.read_parsed_tasks();
            if let Some(active_idx) = tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
            {
                let active_title = &tasks[active_idx].title;
                if tasks
                    .get(active_idx + 1)
                    .is_some_and(|next_t| Self::is_verification_gate_step(&next_t.title))
                    && !Self::step_requires_runtime_verification(active_title)
                    && self.is_step_deliverables_satisfied_on_disk(active_idx, &tasks)
                    && self.update_progress(active_title, "completed").is_ok()
                {
                    reconciled += 1;
                }
            }
            if !self.is_tactical_subplan() {
                if reconciled > 0 {
                    self.record_auto_promoted_active_step();
                }
                return reconciled;
            }
        }

        if !self.is_tactical_subplan() {
            return 0;
        }

        // Stage 1 -> Stage 2: Static build/lint substep -> Live dev server / browser QA substep

        if starts_browser_stage {
            let sub_tasks = self.read_task_plan_tasks();
            if let Some(active_idx) = sub_tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
            {
                if let Some(next_sub) = sub_tasks.get(active_idx + 1) {
                    let active_title = &sub_tasks[active_idx].title;
                    if !Self::task_declares_verifiable_deliverables(active_title)
                        && Self::step_requires_static_build_or_lint(active_title)
                        && !Self::step_requires_browser_verification(active_title)
                        && Self::step_requires_browser_verification(&next_sub.title)
                        && self.update_progress(active_title, "completed").is_ok()
                    {
                        reconciled += 1;
                    }
                }
            }
        }

        // Stage 2 -> Stage 3: Desktop browser QA substep -> Responsive tablet/mobile viewport substep
        if Self::tool_switches_to_responsive_viewport(tool_name, tool_args) {
            let sub_tasks = self.read_task_plan_tasks();
            if let Some(active_idx) = sub_tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
            {
                if let Some(next_sub) = sub_tasks.get(active_idx + 1) {
                    let active_title = &sub_tasks[active_idx].title;
                    let active_lower = Self::strip_parentheticals_lower(active_title);
                    if !Self::task_declares_verifiable_deliverables(active_title)
                        && active_lower.contains("desktop")
                        && !Self::step_mentions_multi_viewport_qa(active_title)
                        && Self::step_mentions_multi_viewport_qa(&next_sub.title)
                        && self.update_progress(active_title, "completed").is_ok()
                    {
                        reconciled += 1;
                    }
                }
            }
        }

        if reconciled > 0 {
            self.record_auto_promoted_active_step();
        }
        reconciled
    }

    /// Returns true when a task step structurally declares concrete file or code-symbol deliverables
    /// (explicit file paths, a `<Category>: <Symbol1>, <Symbol2>` deliverable group, or compound
    /// `CamelCase`/`PascalCase`/`snake_case` symbol identifiers) rather than a verification/command gate.
    pub(crate) fn task_declares_verifiable_deliverables(raw_title: &str) -> bool {
        if !Self::extract_candidate_files(raw_title).is_empty() {
            return true;
        }
        if Self::is_verification_gate_step(raw_title) {
            return false;
        }
        let (_, is_deliverable_group) = Self::parse_deliverable_head(raw_title);
        if is_deliverable_group {
            return true;
        }
        let head_segment = raw_title
            .split('—')
            .next()
            .unwrap_or(raw_title)
            .split(';')
            .next()
            .unwrap_or(raw_title)
            .split(':')
            .next()
            .unwrap_or(raw_title);
        let mut outside_parens = String::with_capacity(head_segment.len());
        let mut depth = 0usize;
        for ch in head_segment.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth == 0 => outside_parens.push(ch),
                _ => {}
            }
        }
        outside_parens
            .split_whitespace()
            .any(Self::has_compound_identifier_morphology)
    }

    /// Returns true if `(rel_path, stem_tokens)` belongs to `task_idx` under bipartite ownership:
    /// not explicitly claimed as a file candidate exclusively by another task, matches `task_idx`'s
    /// primary deliverable head (`head_score > 0`), achieves the global maximum alignment score across
    /// the plan, and strictly exceeds any already-`Completed` task's score.
    fn is_file_owned_by_task(
        rel_path: &str,
        stem_tokens: &[String],
        task_idx: usize,
        tasks: &[TaskItem],
        task_heads: &[String],
        task_lowers: &[String],
        task_explicit_candidates: &[Vec<String>],
    ) -> bool {
        let claimed_by_self = task_explicit_candidates[task_idx]
            .iter()
            .any(|c| Self::paths_match_by_components(rel_path, c));
        if !claimed_by_self
            && task_explicit_candidates
                .iter()
                .enumerate()
                .any(|(other_idx, cands)| {
                    other_idx != task_idx
                        && cands
                            .iter()
                            .any(|c| Self::paths_match_by_components(rel_path, c))
                })
        {
            return false;
        }

        let (head_score, total_score) =
            Self::score_file_to_task(stem_tokens, task_idx, tasks, task_heads, task_lowers);
        if head_score <= 0.0 {
            return false;
        }

        let mut best_global_score = 0.0_f64;
        let mut best_completed_other_score = 0.0_f64;
        for other_idx in 0..tasks.len() {
            if other_idx != task_idx && Self::is_verification_gate_step(&tasks[other_idx].title) {
                continue;
            }
            let (_, other_total) =
                Self::score_file_to_task(stem_tokens, other_idx, tasks, task_heads, task_lowers);
            if other_total > best_global_score {
                best_global_score = other_total;
            }
            if other_idx != task_idx
                && tasks[other_idx].status == TaskItemStatus::Completed
                && other_total > best_completed_other_score
            {
                best_completed_other_score = other_total;
            }
        }

        if best_completed_other_score >= total_score - 1e-6 {
            return false;
        }
        total_score + 1e-6 >= best_global_score
    }

    /// Checks whether all declared deliverables for `tasks[task_idx]` are verified as non-empty,
    /// non-scaffold files on disk under global bipartite task ownership.
    pub(super) fn are_task_deliverables_verified_on_disk(
        &self,
        task_idx: usize,
        tasks: &[TaskItem],
    ) -> bool {
        let Some(task) = tasks.get(task_idx) else {
            return false;
        };
        let candidates = Self::extract_candidate_files(&task.title);
        if !candidates.is_empty() {
            return candidates
                .iter()
                .all(|c| self.is_candidate_completed_on_disk(c));
        }
        if !Self::task_declares_verifiable_deliverables(&task.title) {
            return false;
        }

        let task_lowers: Vec<String> = tasks
            .iter()
            .map(|t| Self::expand_camelcase_to_lower(&t.title))
            .collect();
        let task_heads: Vec<String> = tasks
            .iter()
            .map(|t| Self::extract_primary_deliverable_head(&t.title))
            .collect();
        let task_explicit_candidates: Vec<Vec<String>> = tasks
            .iter()
            .map(|t| Self::extract_candidate_files(&t.title))
            .collect();

        let ws_files = self.collect_workspace_files_with_stems(&[]);
        let owned_files: Vec<(&str, &Vec<String>)> = ws_files
            .iter()
            .filter(|(rel, stems)| {
                Self::is_file_owned_by_task(
                    rel,
                    stems,
                    task_idx,
                    tasks,
                    &task_heads,
                    &task_lowers,
                    &task_explicit_candidates,
                )
            })
            .map(|(rel, stems)| (rel.as_str(), stems))
            .collect();

        if owned_files.is_empty() {
            return false;
        }
        let clauses = Self::split_heading_clauses(&task_heads[task_idx]);
        if clauses.is_empty() {
            return false;
        }
        if clauses.len() == 1 {
            owned_files.iter().any(|(_, stems)| {
                Self::single_clause_segments_align(stems, &clauses[0], &task_lowers[task_idx])
            })
        } else {
            self.are_heading_clauses_covered(&clauses, &owned_files)
        }
    }

    /// Returns true if `clause` consists solely of CSS layout/arrangement container nouns
    /// (such as `"bento grid"`, `"12-col layout"`, or `"masonry grid"`), which are commonly
    /// implemented as grid markup inside a step's domain section component rather than as a
    /// standalone `Bento.tsx` file.
    fn is_pure_layout_geometry_clause(clause: &str) -> bool {
        let mut has_word = false;
        for w in clause.split(|c: char| !c.is_ascii_alphanumeric()) {
            if w.len() < 2 {
                continue;
            }
            has_word = true;
            if !matches!(
                w,
                "bento"
                    | "grid"
                    | "masonry"
                    | "layout"
                    | "columns"
                    | "column"
                    | "col"
                    | "cols"
                    | "split"
                    | "12"
            ) {
                return false;
            }
        }
        has_word
    }

    /// Checks whether every sub-clause in `clauses` is satisfied either by a matching owned file
    /// identifier stem or — for a pure layout-geometry clause paired with at least one domain-matched
    /// clause — by multi-column/bento CSS grid markup inside one of the step's owned files.
    fn are_heading_clauses_covered(
        &self,
        clauses: &[String],
        owned_files: &[(&str, &Vec<String>)],
    ) -> bool {
        let any_stem_clause_matched = clauses.iter().any(|clause| {
            owned_files.iter().any(|(_, file_stems)| {
                file_stems
                    .iter()
                    .any(|tok| Self::stem_token_matches_task(tok, clause))
            })
        });
        if !any_stem_clause_matched {
            return false;
        }
        clauses.iter().all(|clause| {
            let matched_by_stem = owned_files.iter().any(|(_, file_stems)| {
                file_stems
                    .iter()
                    .any(|tok| Self::stem_token_matches_task(tok, clause))
            });
            if matched_by_stem {
                return true;
            }
            if !Self::is_pure_layout_geometry_clause(clause) {
                return false;
            }
            owned_files.iter().any(|(rel, _)| {
                fs::read_to_string(self.workspace_root.join(rel))
                    .map(|content| {
                        let lower = content.to_ascii_lowercase();
                        lower.contains("col-span-")
                            || lower.contains("grid-cols-")
                            || lower.contains("bento")
                            || lower.contains("masonry")
                    })
                    .unwrap_or(false)
            })
        })
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

    /// Extracts distinctive lowercase domain/identifier tokens (`len >= 4`) from an intra-file
    /// substep title, filtering out generic task verbs and container nouns so content verification
    /// against the parent task's target file checks substantive identifiers/properties.
    fn extract_substep_content_tokens(substep_title: &str) -> Vec<String> {
        const GENERIC_SUBSTEP_WORDS: &[&str] = &[
            "reset",
            "rewrite",
            "create",
            "update",
            "implement",
            "build",
            "write",
            "define",
            "configure",
            "setup",
            "ensure",
            "support",
            "apply",
            "wire",
            "variables",
            "variable",
            "classes",
            "class",
            "utility",
            "utilities",
            "imports",
            "import",
            "styles",
            "styling",
            "tokens",
            "token",
            "palette",
            "scheme",
            "hierarchy",
            "structure",
            "handling",
            "without",
            "using",
            "filters",
            "filter",
            "with",
            "from",
            "into",
            "only",
            "none",
        ];
        let expanded = Self::expand_camelcase_to_lower(substep_title);
        let mut out = Vec::new();
        for w in expanded.split(|c: char| !c.is_ascii_alphanumeric()) {
            if w.len() >= 4
                && !Self::is_structural_stem(w)
                && !GENERIC_SUBSTEP_WORDS.contains(&w)
                && !out.iter().any(|existing: &String| existing == w)
            {
                out.push(w.to_string());
            }
        }
        out
    }

    /// Reconciles an active `Mode: subplan` (`task_plan.md`) against its parent `[>]` macro task
    /// and sibling roadmap tasks in canonical `todo.md`:
    /// 1. **Intra-File Substep Content Verification**: When the parent macro task declares explicit
    ///    target file(s) (e.g. `src/index.css`) that were modified and verified on disk, any child
    ///    substep without its own file path is verified against the parent file's on-disk content.
    /// 2. **Cross-Macro Progression Auto-Closure**: When all declared deliverables of the parent
    ///    macro task are verified on disk, no remaining substep declares unverified file deliverables,
    ///    and `modified_files` modifies a deliverable belonging to a subsequent macro task in `todo.md`,
    ///    remaining intra-file prose substeps auto-complete so `task_plan.md` transitions back to
    ///    `Mode: direct` in real time.
    fn reconcile_subplan_against_parent_and_roadmap(&self, modified_files: &[String]) -> usize {
        if !self.is_tactical_subplan() {
            return 0;
        }
        let milestones = self.read_roadmap_milestones();
        let Some(m) = milestones.iter().find(|m| m.is_active) else {
            return 0;
        };
        let Some(parent_idx) = m
            .tasks
            .iter()
            .position(|t| t.status == TaskItemStatus::InProgress)
            .or_else(|| {
                m.tasks
                    .iter()
                    .position(|t| t.status == TaskItemStatus::Pending)
            })
        else {
            return 0;
        };
        let parent_task = &m.tasks[parent_idx];
        let sub_tasks = self.read_task_plan_tasks();
        if sub_tasks.is_empty() {
            return 0;
        }

        let mut reconciled = 0usize;
        let parent_cands = Self::extract_candidate_files(&parent_task.title);

        // 1. Intra-file substep content verification against parent's target file(s)
        if !parent_cands.is_empty()
            && parent_cands
                .iter()
                .all(|c| self.is_candidate_completed_on_disk(c))
            && (modified_files.is_empty()
                || modified_files.iter().any(|m_path| {
                    parent_cands
                        .iter()
                        .any(|c| Self::paths_match_by_components(m_path, c))
                }))
        {
            let mut parent_disk_lower = String::new();
            for cand in &parent_cands {
                if let Some(resolved) = self.resolve_candidate_on_disk(cand) {
                    if let Ok(text) = fs::read_to_string(&resolved) {
                        parent_disk_lower.push_str(&text.to_ascii_lowercase());
                        parent_disk_lower.push('\n');
                    }
                }
            }
            if !parent_disk_lower.is_empty() {
                for sub in &sub_tasks {
                    if sub.status == TaskItemStatus::Completed
                        || !Self::extract_candidate_files(&sub.title).is_empty()
                        || Self::is_verification_gate_step(&sub.title)
                    {
                        continue;
                    }
                    let tokens = Self::extract_substep_content_tokens(&sub.title);
                    if tokens.is_empty() {
                        continue;
                    }
                    let matched = tokens
                        .iter()
                        .filter(|tok| parent_disk_lower.contains(tok.as_str()))
                        .count();
                    if matched * 2 >= tokens.len()
                        && self.update_progress(&sub.title, "completed").is_ok()
                    {
                        reconciled += 1;
                    }
                }
            }
        }

        // 2. Cross-macro progression auto-closure when parent deliverables are done and a sibling macro task's file is modified
        if self.is_tactical_subplan() && !modified_files.is_empty() {
            let remaining_subs: Vec<TaskItem> = self
                .read_task_plan_tasks()
                .into_iter()
                .filter(|s| s.status != TaskItemStatus::Completed)
                .collect();
            let parent_verified = Self::task_declares_verifiable_deliverables(&parent_task.title)
                && self.are_task_deliverables_verified_on_disk(parent_idx, &m.tasks);
            let subs_are_pure_intra_prose = !remaining_subs.is_empty()
                && remaining_subs.iter().all(|s| {
                    !Self::is_verification_gate_step(&s.title)
                        && !Self::step_requires_runtime_verification(&s.title)
                        && Self::extract_candidate_files(&s.title)
                            .iter()
                            .all(|c| self.is_candidate_completed_on_disk(c))
                });

            if parent_verified && subs_are_pure_intra_prose {
                let macro_lowers: Vec<String> = m
                    .tasks
                    .iter()
                    .map(|t| Self::expand_camelcase_to_lower(&t.title))
                    .collect();
                let macro_heads: Vec<String> = m
                    .tasks
                    .iter()
                    .map(|t| Self::extract_primary_deliverable_head(&t.title))
                    .collect();
                let macro_cands: Vec<Vec<String>> = m
                    .tasks
                    .iter()
                    .map(|t| Self::extract_candidate_files(&t.title))
                    .collect();

                let moved_to_sibling_macro = modified_files.iter().any(|m_path| {
                    let claimed_by_parent = macro_cands[parent_idx]
                        .iter()
                        .any(|c| Self::paths_match_by_components(m_path, c));
                    if claimed_by_parent {
                        return false;
                    }
                    let explicitly_claimed_by_other =
                        macro_cands.iter().enumerate().any(|(idx, cands)| {
                            idx != parent_idx
                                && cands
                                    .iter()
                                    .any(|c| Self::paths_match_by_components(m_path, c))
                        });
                    if explicitly_claimed_by_other {
                        return true;
                    }
                    let stems = Self::split_identifier_stem_tokens(m_path);
                    !stems.is_empty()
                        && (0..m.tasks.len()).any(|idx| {
                            idx != parent_idx
                                && !Self::is_verification_gate_step(&m.tasks[idx].title)
                                && Self::is_file_owned_by_task(
                                    m_path,
                                    &stems,
                                    idx,
                                    &m.tasks,
                                    &macro_heads,
                                    &macro_lowers,
                                    &macro_cands,
                                )
                        })
                });

                if moved_to_sibling_macro {
                    for sub in remaining_subs {
                        if self.update_progress(&sub.title, "completed").is_ok() {
                            reconciled += 1;
                        }
                    }
                }
            }
        }

        reconciled
    }

    /// Dynamically reconciles uncompleted tasks against files modified during the turn
    /// or verified on disk in the workspace using explicit deliverable verification and
    /// global IDF-weighted bipartite best-owner attribution.
    ///
    /// Returns the number of tasks updated to `Completed`.
    pub fn reconcile_workspace_tasks(&self, modified_files: &[String]) -> usize {
        let mut reconciled = self.reconcile_subplan_against_parent_and_roadmap(modified_files);
        let tasks = self.read_parsed_tasks();
        if tasks.is_empty() {
            return reconciled;
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

        let task_lowers: Vec<String> = tasks
            .iter()
            .map(|t| Self::expand_camelcase_to_lower(&t.title))
            .collect();
        let task_heads: Vec<String> = tasks
            .iter()
            .map(|t| Self::extract_primary_deliverable_head(&t.title))
            .collect();
        let task_explicit_candidates: Vec<Vec<String>> = tasks
            .iter()
            .map(|t| Self::extract_candidate_files(&t.title))
            .collect();

        let mut consumed_modified: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        let mut workspace_files_cache: Option<Vec<(String, Vec<String>)>> = None;

        for (task_idx, task) in tasks.iter().enumerate() {
            if task.status == TaskItemStatus::Completed {
                continue;
            }

            let primary_head = &task_heads[task_idx];
            let candidates = &task_explicit_candidates[task_idx];
            let requires_runtime_verify = Self::step_requires_runtime_verification(&task.title);
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
                        consumed_modified.insert(m_idx);
                        if !requires_runtime_verify {
                            file_satisfied = true;
                        }
                    }
                } else if !requires_runtime_verify
                    && task.status == TaskItemStatus::InProgress
                    && self.is_candidate_completed_on_disk(candidate)
                {
                    file_satisfied = true;
                }
            } else if candidates.len() > 1 {
                let mut all_candidates_ready = true;
                let mut any_modified_match = false;
                for candidate in candidates {
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
                if all_candidates_ready && !requires_runtime_verify {
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
            // specific file or symbol deliverables and a root project manifest was created/modified, mark it satisfied.
            if !file_satisfied
                && candidates.is_empty()
                && task_idx == 0
                && task.status == TaskItemStatus::InProgress
                && !Self::task_declares_verifiable_deliverables(&task.title)
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
                && !requires_runtime_verify
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
            // 1. `m_path` is not explicitly claimed as a file candidate exclusively by another task.
            // 2. `task_idx` matches the file in its primary deliverable heading (`head_score > 0.0`).
            // 3. `task_idx` achieves the global maximum alignment score across ALL tasks in the plan
            //    (including already-Completed tasks, preventing re-edits of an earlier task's file
            //    from spilling over to a later task).
            // 4. Every deliverable sub-clause of `primary_head` is covered by workspace files owned by `task_idx`.
            if !file_satisfied
                && !requires_runtime_verify
                && candidates.is_empty()
                && !modified_files.is_empty()
            {
                for (m_idx, m_path) in modified_files.iter().enumerate() {
                    if consumed_modified.contains(&m_idx) {
                        continue;
                    }
                    if task_explicit_candidates
                        .iter()
                        .enumerate()
                        .any(|(other_idx, cands)| {
                            other_idx != task_idx
                                && cands
                                    .iter()
                                    .any(|c| Self::paths_match_by_components(m_path, c))
                        })
                    {
                        consumed_modified.insert(m_idx);
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
                        if other_idx != task_idx
                            && Self::is_verification_gate_step(&tasks[other_idx].title)
                        {
                            continue;
                        }
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

                    // Sub-clause and multi-segment alignment check:
                    let clauses = Self::split_heading_clauses(primary_head);
                    if clauses.len() == 1 {
                        if !Self::single_clause_segments_align(
                            &stem_tokens,
                            &clauses[0],
                            &task_lowers[task_idx],
                        ) {
                            continue;
                        }
                    } else if clauses.len() > 1 {
                        let ws_files = workspace_files_cache.get_or_insert_with(|| {
                            self.collect_workspace_files_with_stems(modified_files)
                        });
                        let owned_files: Vec<(&str, &Vec<String>)> = ws_files
                            .iter()
                            .filter(|(rel, stems)| {
                                Self::is_file_owned_by_task(
                                    rel,
                                    stems,
                                    task_idx,
                                    &tasks,
                                    &task_heads,
                                    &task_lowers,
                                    &task_explicit_candidates,
                                )
                            })
                            .map(|(rel, stems)| (rel.as_str(), stems))
                            .collect();
                        let all_clauses_covered =
                            self.are_heading_clauses_covered(&clauses, &owned_files);
                        if !all_clauses_covered {
                            consumed_modified.insert(m_idx);
                            continue;
                        }
                    }

                    file_satisfied = true;
                    consumed_modified.insert(m_idx);
                    break;
                }
            } else if !file_satisfied
                && !requires_runtime_verify
                && candidates.is_empty()
                && modified_files.is_empty()
            {
                // Disk-only verification pass (`reconcile_verification_tasks`): reconcile symbol/group
                // deliverable tasks whose owned workspace files are all verified on disk.
                if self.are_task_deliverables_verified_on_disk(task_idx, &tasks) {
                    file_satisfied = true;
                }
            }

            if file_satisfied {
                if let Ok(()) = self.update_progress(&task.title, "completed") {
                    reconciled += 1;
                }
            }
        }

        if reconciled > 0 {
            if let Some(active) = self
                .read_parsed_tasks()
                .into_iter()
                .find(|t| t.status == TaskItemStatus::InProgress)
            {
                if Self::task_declares_verifiable_deliverables(&active.title) {
                    self.record_auto_promoted_active_step();
                }
            }
        }

        if self.is_all_project_tasks_completed() {
            let p_path = self.progress_path();
            if let Ok(content) = fs::read_to_string(&p_path) {
                if content.contains("- Status: In Progress") {
                    let updated =
                        content.replacen("- Status: In Progress", "- Status: Completed (100%)", 1);
                    let _ = fs::write(&p_path, updated);
                }
            }
            self.sync_completed_core_docs();
        }

        reconciled
    }

    /// Reconciles any remaining deliverable plan tasks whose files are already present on disk
    /// when `power_verify` runs.
    pub fn reconcile_verification_tasks(&self) -> usize {
        self.reconcile_workspace_tasks(&[])
    }

    /// Checks whether `tasks[task_idx]`'s file/DOM/foundation deliverables exist on disk
    /// (independent of whether the step additionally requires runtime verification).
    fn is_step_deliverables_satisfied_on_disk(&self, task_idx: usize, tasks: &[TaskItem]) -> bool {
        let Some(task) = tasks.get(task_idx) else {
            return false;
        };
        if self.are_task_deliverables_verified_on_disk(task_idx, tasks) {
            return true;
        }
        let candidates = Self::extract_candidate_files(&task.title);
        if candidates.is_empty()
            && task_idx == 0
            && !Self::task_declares_verifiable_deliverables(&task.title)
            && !Self::step_requires_runtime_verification(&task.title)
            && crate::agent::orchestrator::WorkflowRouter::project_manifest(&self.workspace_root)
                .is_some()
        {
            return true;
        }
        let has_non_html_candidates = candidates
            .iter()
            .any(|c| !c.ends_with(".html") && !c.ends_with(".htm"));
        if !has_non_html_candidates && !Self::is_verification_gate_step(&task.title) {
            let index_html = self.workspace_root.join("index.html");
            if index_html.is_file() && !self.is_unmodified_scaffold_file(&index_html) {
                if let Ok(content) = fs::read_to_string(&index_html) {
                    let dom_ids = Self::extract_html_section_ids(&content);
                    if !dom_ids.is_empty() {
                        let primary_head = Self::extract_primary_deliverable_head(&task.title);
                        let clauses = Self::split_heading_clauses(&primary_head);
                        if !clauses.is_empty()
                            && clauses.iter().all(|clause| {
                                dom_ids
                                    .iter()
                                    .any(|sec_id| Self::stem_token_matches_task(sec_id, clause))
                            })
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Evaluates whether the currently active `[>]` step's file deliverables are ready on disk so
    /// the tool loop can prompt the agent to call `update_progress` immediately after each step
    /// (or finish runtime verification first on compound steps) instead of deferring to the end.
    pub fn active_step_readiness(&self) -> ActiveStepReadiness {
        let tasks = self.read_parsed_tasks();
        let Some(active_idx) = tasks
            .iter()
            .position(|t| t.status == TaskItemStatus::InProgress)
        else {
            return ActiveStepReadiness::InProgress;
        };
        if !self.is_step_deliverables_satisfied_on_disk(active_idx, &tasks) {
            return ActiveStepReadiness::InProgress;
        }
        if Self::step_requires_runtime_verification(&tasks[active_idx].title) {
            ActiveStepReadiness::AwaitingVerification
        } else {
            ActiveStepReadiness::DeliverablesReady
        }
    }

    /// Mid-turn transition catch-up:
    /// - Keeps the currently active step `[>]` `InProgress` while the agent is actively building its
    ///   files so the agent explicitly calls `update_progress` at the end of each step (visible in TUI).
    /// - If the agent skips calling `update_progress` on step `k` and begins modifying a file owned by
    ///   a later step `target_idx > k`, automatically marks completed preceding steps `0..target_idx`
    ///   as `[x]` so `todo.md` never freezes at `0/N` or batches `update_progress` calls at the end.
    pub fn reconcile_preceding_tasks_on_step_transition(&self, modified_files: &[String]) -> usize {
        let mut reconciled = self.reconcile_subplan_against_parent_and_roadmap(modified_files);
        if modified_files.is_empty() {
            return reconciled;
        }
        let tasks = self.read_parsed_tasks();
        if tasks.is_empty() {
            return reconciled;
        }
        let Some(first_uncompleted) = tasks
            .iter()
            .position(|t| t.status != TaskItemStatus::Completed)
        else {
            return reconciled;
        };

        let task_lowers: Vec<String> = tasks
            .iter()
            .map(|t| Self::expand_camelcase_to_lower(&t.title))
            .collect();
        let task_heads: Vec<String> = tasks
            .iter()
            .map(|t| Self::extract_primary_deliverable_head(&t.title))
            .collect();
        let task_explicit_candidates: Vec<Vec<String>> = tasks
            .iter()
            .map(|t| Self::extract_candidate_files(&t.title))
            .collect();

        let mut html_section_ids = std::collections::HashSet::new();
        for m_path in modified_files {
            if m_path.ends_with(".html") || m_path.ends_with(".htm") {
                let full_path = self.workspace_root.join(m_path.trim_start_matches("./"));
                if let Ok(content) = fs::read_to_string(&full_path) {
                    html_section_ids.extend(Self::extract_html_section_ids(&content));
                }
            }
        }

        let mut max_transition_target: Option<usize> = None;

        for m_path in modified_files {
            let is_html = m_path.ends_with(".html") || m_path.ends_with(".htm");
            let claimed_by_active = !is_html
                && task_explicit_candidates[first_uncompleted]
                    .iter()
                    .any(|c| Self::paths_match_by_components(m_path, c));
            if claimed_by_active {
                continue;
            }

            // 1. Check explicit file candidates on later steps `target_idx > first_uncompleted`
            let mut matched_explicit_later = false;
            for (target_idx, cands) in task_explicit_candidates
                .iter()
                .enumerate()
                .skip(first_uncompleted + 1)
            {
                if cands
                    .iter()
                    .any(|c| Self::paths_match_by_components(m_path, c))
                {
                    max_transition_target =
                        Some(max_transition_target.map_or(target_idx, |cur| cur.max(target_idx)));
                    matched_explicit_later = true;
                }
            }
            if matched_explicit_later {
                continue;
            }

            // Skip if an already-completed earlier step explicitly claims this file
            if task_explicit_candidates[..first_uncompleted]
                .iter()
                .any(|cands| {
                    cands
                        .iter()
                        .any(|c| Self::paths_match_by_components(m_path, c))
                })
            {
                continue;
            }

            // 2. Single-file HTML `<section id="...">` transition detection
            if is_html && !html_section_ids.is_empty() {
                for target_idx in (first_uncompleted + 1)..tasks.len() {
                    if Self::is_verification_gate_step(&tasks[target_idx].title) {
                        continue;
                    }
                    let has_non_html = task_explicit_candidates[target_idx]
                        .iter()
                        .any(|c| !c.ends_with(".html") && !c.ends_with(".htm"));
                    if has_non_html {
                        continue;
                    }
                    let clauses = Self::split_heading_clauses(&task_heads[target_idx]);
                    if !clauses.is_empty()
                        && clauses.iter().all(|clause| {
                            html_section_ids
                                .iter()
                                .any(|sec_id| Self::stem_token_matches_task(sec_id, clause))
                        })
                    {
                        max_transition_target = Some(
                            max_transition_target.map_or(target_idx, |cur| cur.max(target_idx)),
                        );
                    }
                }
                continue;
            }

            // 3. Global IDF-weighted bipartite ownership on later steps `target_idx > first_uncompleted`
            let stem_tokens = Self::split_identifier_stem_tokens(m_path);
            if stem_tokens.is_empty() {
                continue;
            }
            for target_idx in (first_uncompleted + 1)..tasks.len() {
                if !task_explicit_candidates[target_idx].is_empty()
                    || Self::is_verification_gate_step(&tasks[target_idx].title)
                {
                    continue;
                }
                if !Self::is_file_owned_by_task(
                    m_path,
                    &stem_tokens,
                    target_idx,
                    &tasks,
                    &task_heads,
                    &task_lowers,
                    &task_explicit_candidates,
                ) {
                    continue;
                }
                let clauses = Self::split_heading_clauses(&task_heads[target_idx]);
                if clauses.len() == 1
                    && !Self::single_clause_segments_align(
                        &stem_tokens,
                        &clauses[0],
                        &task_lowers[target_idx],
                    )
                {
                    continue;
                }
                max_transition_target =
                    Some(max_transition_target.map_or(target_idx, |cur| cur.max(target_idx)));
            }
        }

        if let Some(target_idx) = max_transition_target {
            for (prev_idx, prev_task) in tasks
                .iter()
                .enumerate()
                .take(target_idx)
                .skip(first_uncompleted)
            {
                if prev_task.status == TaskItemStatus::Completed
                    || Self::step_requires_runtime_verification(&prev_task.title)
                {
                    continue;
                }
                if self.is_step_deliverables_satisfied_on_disk(prev_idx, &tasks)
                    && self.update_progress(&prev_task.title, "completed").is_ok()
                {
                    reconciled += 1;
                }
            }
        }

        reconciled
    }

    /// Formats a concise status hint showing how many plan tasks are `[x] Completed` and listing
    /// ONLY the remaining uncompleted task numbers/titles (if any) so the agent never wastes turns
    /// calling `update_progress` on steps that were already auto-reconciled to `[x]`.
    pub fn format_remaining_tasks_hint(&self) -> String {
        let tasks = self.read_parsed_tasks();
        if tasks.is_empty() {
            return String::new();
        }
        let macro_prefix = if self.is_tactical_subplan() {
            self.read_roadmap_milestones()
                .into_iter()
                .find(|m| m.is_active && m.tasks.len() > 1)
                .and_then(|m| {
                    let macro_done = m
                        .tasks
                        .iter()
                        .filter(|t| t.status == TaskItemStatus::Completed)
                        .count();
                    let macro_total = m.tasks.len();
                    m.tasks
                        .iter()
                        .position(|t| t.status == TaskItemStatus::InProgress)
                        .map(|pos| {
                            format!(
                                "Macro Roadmap: {}/{} completed (Active Step #{}: \"{}\") | Sub-Plan: ",
                                macro_done,
                                macro_total,
                                pos + 1,
                                m.tasks[pos].title
                            )
                        })
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        let total = tasks.len();
        let remaining: Vec<(usize, &TaskItem)> = tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status != TaskItemStatus::Completed)
            .collect();
        let completed = total.saturating_sub(remaining.len());

        if remaining.is_empty() {
            format!(
                "📋 Plan Status: {}All {}/{} tasks are already marked `[x]` Completed. Do NOT call `update_progress` — call `archive_plan` directly when ready to finalize.",
                macro_prefix, total, total
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
                "📋 Plan Status: {}{}/{} tasks `[x]` Completed{}. Remaining uncompleted step(s): {}{}",
                macro_prefix, completed, total, done_clause, rem_list, suffix
            )
        }
    }
}
