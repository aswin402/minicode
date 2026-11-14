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

    /// Strips a trailing generic structural container suffix (`Section`, `Component`, `Wrapper`, `Container`,
    /// or `-section` / `_section`) when preceded by an identifier boundary (CamelCase uppercase transition or `-`/`_`),
    /// normalizing e.g. `BrowserSection` -> `Browser` while keeping `BrowserHud` -> `BrowserHud`.
    pub(crate) fn strip_trailing_structural_container_stem(stem: &str) -> &str {
        let trimmed = stem.trim();
        if trimmed.is_empty() {
            return trimmed;
        }
        for &suffix in crate::constants::STRUCTURAL_CONTAINER_STEMS {
            if trimmed.len() <= suffix.len() {
                continue;
            }
            let split_at = trimmed.len() - suffix.len();
            if !trimmed.is_char_boundary(split_at) {
                continue;
            }
            let tail = &trimmed[split_at..];
            if !tail.eq_ignore_ascii_case(suffix) {
                continue;
            }
            let head = &trimmed[..split_at];
            if let Some(stripped_head) = head.strip_suffix(['-', '_']) {
                if stripped_head.len() >= 2 {
                    return stripped_head;
                }
            } else if tail.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && head.chars().any(|c| c.is_ascii_lowercase())
                && head.len() >= 2
            {
                return head;
            }
        }
        trimmed
    }

    /// Checks whether `path` ends with `suffix` when allowing structural container-stem equivalence
    /// (`Browser.tsx` <-> `src/components/sections/BrowserSection.tsx`) with identical extension and parent suffix.
    fn path_ends_with_structural_container_equiv(path: &Path, suffix: &Path) -> bool {
        let Some(ext_a) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let Some(ext_b) = suffix.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        if !ext_a.eq_ignore_ascii_case(ext_b) {
            return false;
        }
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
        let parent_len = suffix_comps.len() - 1;
        if !path_comps[offset..offset + parent_len]
            .iter()
            .zip(suffix_comps[..parent_len].iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return false;
        }
        let Some(stem_a) = path.file_stem().and_then(|s| s.to_str()) else {
            return false;
        };
        let Some(stem_b) = suffix.file_stem().and_then(|s| s.to_str()) else {
            return false;
        };
        let norm_a = Self::strip_trailing_structural_container_stem(stem_a);
        let norm_b = Self::strip_trailing_structural_container_stem(stem_b);
        !norm_a.is_empty() && norm_a.eq_ignore_ascii_case(norm_b)
    }

    /// Compares two relative file paths by their `std::path::Path` component hierarchy,
    /// returning true if they are equal or one is a component-aligned suffix of the other
    /// (e.g. `sections/Swarm.tsx` matches `src/components/sections/Swarm.tsx`, `Browser.tsx` matches `BrowserSection.tsx`).
    pub(crate) fn paths_match_by_components(a: &str, b: &str) -> bool {
        let clean_a = a.trim_start_matches("./");
        let clean_b = b.trim_start_matches("./");
        if clean_a.eq_ignore_ascii_case(clean_b) {
            return true;
        }
        let pa = Path::new(clean_a);
        let pb = Path::new(clean_b);
        Self::path_ends_with_ignore_ascii_case(pa, pb)
            || Self::path_ends_with_ignore_ascii_case(pb, pa)
            || Self::path_ends_with_structural_container_equiv(pa, pb)
            || Self::path_ends_with_structural_container_equiv(pb, pa)
    }

    /// Resolves a candidate file path, shorthand subpath (`sections/Swarm.tsx`), or bare filename
    /// (`Swarm.tsx` / `CTA.tsx` / `Browser.tsx`) to an existing non-empty file on disk via component suffix matching.
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
        let mut structural_fallback: Option<PathBuf> = None;
        for entry in walker.flatten() {
            let p = entry.path();
            if p.is_file() && fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false) {
                if Self::path_ends_with_ignore_ascii_case(p, cand_path) {
                    return Some(p.to_path_buf());
                }
                if structural_fallback.is_none()
                    && Self::path_ends_with_structural_container_equiv(p, cand_path)
                {
                    structural_fallback = Some(p.to_path_buf());
                }
            }
        }
        structural_fallback
    }

    /// Returns true if `path` on disk still contains the exact unmodified content of a built-in scaffold template file.
    pub(crate) fn is_unmodified_scaffold_file(&self, path: &Path) -> bool {
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
    pub(crate) fn is_structural_stem(word: &str) -> bool {
        crate::constants::STRUCTURAL_CONTAINER_STEMS.contains(&word)
    }

    /// Reduces an English identifier word to its root stem by stripping common derivational/inflectional suffixes.
    pub(crate) fn morphological_root(word: &str) -> &str {
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

    /// Extracts deduplicated domain-bearing lowercase tokens (>= 3 chars, CamelCase-expanded,
    /// structural container stems excluded) for cross-phase / cross-milestone coverage comparison.
    pub(crate) fn extract_domain_tokens(text: &str) -> Vec<String> {
        const FUNCTION_WORDS: &[&str] = &[
            "the", "and", "for", "with", "via", "from", "into", "onto", "over", "under", "all",
            "any", "per", "set", "use", "using", "new", "add", "run",
        ];
        let expanded = Self::expand_camelcase_to_lower(text);
        let mut out = Vec::new();
        for tok in expanded
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 3)
        {
            if Self::is_structural_stem(tok) || FUNCTION_WORDS.contains(&tok) {
                continue;
            }
            if !out.iter().any(|existing: &String| {
                existing == tok
                    || Self::morphological_root(existing) == Self::morphological_root(tok)
            }) {
                out.push(tok.to_string());
            }
        }
        out
    }

    /// Returns whether a stem token `s` matches any alphanumeric word `w` in `task_lower`.
    pub(crate) fn stem_token_matches_task(s: &str, task_lower: &str) -> bool {
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
                    || (matches!(s, "nav" | "navbar" | "navigation" | "header" | "topbar")
                        && matches!(w, "nav" | "navbar" | "navigation" | "header" | "topbar"))
            })
    }

    /// Checks whether a multi-segment identifier `file_stems` has sufficient segment alignment
    /// to complete a single-clause task `clause` / `task_lower` on its own.
    /// When `clause` has the form `<Subject> with <Companion>` (e.g. `"Hero with live terminal replay"`),
    /// requires at least one segment of `file_stems` to match the primary `<Subject>` (`"hero"`) so a
    /// companion helper (`TerminalPreview.tsx` or `src/data/terminal.ts`) cannot complete the step
    /// before the primary subject deliverable exists.
    /// When both the file stem has `>= 2` segments and the clause has `>= 2` non-structural words,
    /// requires at least 2 segments of the file to match `task_lower` so a 2-segment helper whose
    /// second segment diverges from the clause (e.g. `CodeBlock.tsx` vs `"Code Intelligence"`)
    /// does not prematurely complete the task on a single-word overlap.
    fn single_clause_segments_align(file_stems: &[String], clause: &str, task_lower: &str) -> bool {
        if let Some((subject, _)) = clause.split_once(" with ") {
            let subject_has_token = subject
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|w| w.len() >= 3 && !Self::is_structural_stem(w));
            if subject_has_token
                && !file_stems
                    .iter()
                    .any(|tok| Self::stem_token_matches_task(tok, subject))
            {
                return false;
            }
        }
        let domain_stems: Vec<&String> = if file_stems.len() >= 2 {
            let stripped: Vec<&String> = file_stems
                .iter()
                .filter(|s| !matches!(s.as_str(), "site" | "app" | "page" | "main" | "root"))
                .collect();
            if stripped.is_empty() {
                file_stems.iter().collect()
            } else {
                stripped
            }
        } else {
            file_stems.iter().collect()
        };
        let clause_word_count = clause
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 3 && !Self::is_structural_stem(w))
            .count();
        if domain_stems.len() >= 2 && clause_word_count >= 2 {
            let matched_segments = domain_stems
                .iter()
                .filter(|tok| Self::stem_token_matches_task(tok, task_lower))
                .count();
            return matched_segments >= 2;
        }
        domain_stems.iter().any(|tok| {
            Self::stem_token_matches_task(tok, clause)
                || (task_lower.contains('(') && Self::stem_token_matches_task(tok, task_lower))
        })
    }

    /// Returns true when `rel_path` is a non-UI data/constants/types fixture file (`src/data/...`,
    /// `data/...`, `src/constants/...`, `src/fixtures/...`, `src/mocks/...`, `types.ts`) while
    /// `clause` does not explicitly ask for data, types, schemas, constants, mocks, or fixtures.
    /// Prevents `src/data/terminal.ts` or `src/data/philosophy.ts` from prematurely satisfying a UI
    /// component/section clause before the rendered `.tsx`/`.jsx`/`.vue`/`.svelte` component exists.
    fn is_pure_data_helper_for_clause(rel_path: &str, clause: &str) -> bool {
        let norm = rel_path.trim_start_matches("./").to_ascii_lowercase();
        let ext = Path::new(&norm)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if matches!(
            ext,
            "tsx" | "jsx" | "vue" | "svelte" | "astro" | "html" | "htm" | "css" | "scss"
        ) {
            return false;
        }
        let in_data_dir = norm.starts_with("src/data/")
            || norm.starts_with("data/")
            || norm.contains("/data/")
            || norm.starts_with("src/constants/")
            || norm.starts_with("src/fixtures/")
            || norm.starts_with("src/mocks/")
            || norm.ends_with("/types.ts")
            || norm == "src/types.ts"
            || norm == "types.ts";
        if !in_data_dir {
            return false;
        }
        !clause.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
            matches!(
                w,
                "data"
                    | "dataset"
                    | "datasets"
                    | "type"
                    | "types"
                    | "schema"
                    | "schemas"
                    | "constant"
                    | "constants"
                    | "fixture"
                    | "fixtures"
                    | "mock"
                    | "mocks"
                    | "seed"
                    | "catalog"
            )
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
    /// verification verb/tool rather than naming a concrete code/UI deliverable artifact.
    pub(crate) fn clause_starts_with_operational_verb(clause: &str) -> bool {
        let first_tok = clause
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        if first_tok.contains('_') && !Self::has_compound_identifier_morphology(first_tok) {
            return true;
        }
        let Some(first) = clause
            .split(|c: char| !c.is_ascii_alphanumeric())
            .find(|w| w.len() >= 2)
            .map(|w| w.to_ascii_lowercase())
        else {
            return true;
        };
        Self::is_verification_phase_word(&first)
            || matches!(
                first.as_str(),
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
                    | "bun"
                    | "npm"
                    | "pnpm"
                    | "yarn"
                    | "npx"
                    | "cargo"
                    | "pip"
                    | "fix"
                    | "fixes"
                    | "resolve"
                    | "remediate"
                    | "repair"
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
        if clauses.len() < 2
            || clauses
                .iter()
                .any(|c| Self::clause_starts_with_operational_verb(c))
        {
            return false;
        }
        if Self::extract_candidate_files(raw_title).is_empty()
            && Self::step_requires_browser_verification(raw_title)
            && clauses.iter().any(|c| {
                c.split(|ch: char| !ch.is_ascii_alphanumeric())
                    .find(|w| w.len() >= 2)
                    .is_some_and(|first| {
                        matches!(
                            first,
                            "desktop"
                                | "mobile"
                                | "tablet"
                                | "viewport"
                                | "viewports"
                                | "screenshot"
                                | "screenshots"
                        )
                    })
            })
        {
            return false;
        }
        true
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

        // Handle `<Subject> with <lowercase item 1>, <item 2>, ...` (e.g. `"Footer with docs, CLI ref, repo links"`)
        // so internal feature lists after `" with "` are not mistaken for separate deliverable files,
        // while preserving capitalized multi-deliverable lists (`"Build Navbar with toggle, Hero, and Footer"`).
        if !before_dash.contains(" + ") && before_dash.contains(", ") {
            let comma_parts: Vec<&str> = before_dash.split(", ").map(str::trim).collect();
            if comma_parts.len() >= 2 {
                let first_lower = comma_parts[0].to_ascii_lowercase();
                if let Some(with_pos) = first_lower.find(" with ") {
                    let subject = comma_parts[0][..with_pos].trim();
                    let has_lowercase_feature_item = comma_parts[1..].iter().any(|part| {
                        let trimmed = part
                            .strip_prefix("and ")
                            .or_else(|| part.strip_prefix("or "))
                            .unwrap_or(part)
                            .trim();
                        !trimmed.contains('.')
                            && trimmed
                                .chars()
                                .find(|ch| ch.is_ascii_alphanumeric())
                                .is_some_and(|ch| ch.is_ascii_lowercase())
                    });
                    if !subject.is_empty() && has_lowercase_feature_item {
                        return (Self::expand_camelcase_to_lower(subject), false);
                    }
                }
            }
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
    /// `AcronymPascal`, or `snake_case`) rather than plain single-case English prose or built-in tool names.
    fn has_compound_identifier_morphology(token: &str) -> bool {
        let clean = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        if clean.len() < 4 {
            return false;
        }
        let lower = clean.to_ascii_lowercase();
        if lower.starts_with("browser_")
            || lower.starts_with("power_")
            || lower.starts_with("minipower_")
            || matches!(
                lower.as_str(),
                "exec_cmd"
                    | "kit_stack_add"
                    | "update_progress"
                    | "archive_plan"
                    | "create_plan"
                    | "patch_file"
                    | "write_file"
                    | "read_file"
            )
        {
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

    /// Returns true when `words` starts with `run` or a CLI runner (`bun`, `npm`, `cargo`, etc.)
    /// and invokes a static verification subcommand (`lint`, `build`, `test`, `typecheck`, `clippy`, `tsc`),
    /// while excluding dependency-installation commands (`bun add ...`, `npm install ...`).
    fn is_cli_verification_clause(words: &[&str]) -> bool {
        let Some(&first) = words.first() else {
            return false;
        };
        let starts_with_runner = matches!(
            first,
            "run"
                | "bun"
                | "npm"
                | "pnpm"
                | "yarn"
                | "npx"
                | "cargo"
                | "uv"
                | "python"
                | "pytest"
                | "vitest"
                | "jest"
                | "eslint"
                | "tsc"
        );
        starts_with_runner
            && words.iter().any(|&w| {
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
                        | "jest"
                        | "eslint"
                )
            })
    }

    /// Returns true if `raw_title` is a command/browser/QA verification gate without explicit file deliverables.
    fn is_verification_gate_step(raw_title: &str) -> bool {
        if !Self::extract_candidate_files(raw_title).is_empty() {
            return false;
        }
        let unparenthesized = Self::strip_parentheticals_lower(raw_title);
        let words: Vec<&str> = unparenthesized
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 2)
            .collect();
        let Some(&first_word) = words.first() else {
            return false;
        };
        let run_invokes_verification = first_word == "run"
            && (Self::is_cli_verification_clause(&words)
                || words[1..].iter().any(|&w| {
                    Self::is_verification_phase_word(w)
                        || matches!(
                            w,
                            "browser"
                                | "playwright"
                                | "lighthouse"
                                | "reproducer"
                                | "checks"
                                | "smoke"
                                | "e2e"
                                | "regression"
                        )
                }));
        let qualified_verification_heading = matches!(
            first_word,
            "responsive"
                | "visual"
                | "final"
                | "full"
                | "complete"
                | "browser"
                | "desktop"
                | "mobile"
                | "tablet"
                | "runtime"
                | "static"
                | "end"
        ) && words.iter().skip(1).take(4).any(|&w| {
            Self::is_verification_phase_word(w)
                || matches!(w, "test" | "tests" | "check" | "checks")
        });
        Self::is_verification_phase_word(first_word)
            || matches!(first_word, "test" | "check")
            || qualified_verification_heading
            || run_invokes_verification
            || Self::is_cli_verification_clause(&words)
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
        let lower_unparenthesized = Self::strip_parentheticals_lower(raw_title);
        for tool_cue in [
            "browser_navigate",
            "browser_screenshot",
            "browser_qa_audit",
            "browser_evaluate",
            "browser_emulate",
            "browser_batch",
            "power_verify",
            "power_review",
        ] {
            if lower_unparenthesized.contains(tool_cue) {
                return true;
            }
        }

        let clause_has_verify_target = |words: &[&str]| -> bool {
            words[1..].iter().any(|&w| {
                matches!(
                    w,
                    "build"
                        | "compile"
                        | "compiles"
                        | "lint"
                        | "tsc"
                        | "typecheck"
                        | "test"
                        | "tests"
                        | "browser"
                        | "viewport"
                        | "viewports"
                        | "screenshot"
                        | "screenshots"
                        | "responsive"
                        | "desktop"
                        | "mobile"
                        | "tablet"
                        | "overflow"
                        | "console"
                        | "localhost"
                        | "dev"
                )
            })
        };

        for statement_seg in lower_unparenthesized.split([';', '—']) {
            let parts: Vec<(&str, bool)> = if let Some((lhs, rhs)) = statement_seg.split_once(':') {
                let lhs_has_verify_cue = lhs
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .filter(|w| w.len() >= 2)
                    .any(|w| {
                        Self::is_verification_phase_word(w)
                            || matches!(
                                w,
                                "test"
                                    | "tests"
                                    | "check"
                                    | "checks"
                                    | "build"
                                    | "browser"
                                    | "responsive"
                                    | "final"
                                    | "integration"
                                    | "polish"
                                    | "closeout"
                            )
                    });
                vec![(lhs, false), (rhs, !lhs_has_verify_cue)]
            } else {
                vec![(statement_seg, false)]
            };

            for (part, in_component_colon_rhs) in parts {
                for (comma_idx, comma_seg) in part.split(',').enumerate() {
                    for chain_seg in comma_seg.split(" && ") {
                        for sub_clause in chain_seg.split(" and ") {
                            let words: Vec<&str> = sub_clause
                                .split(|c: char| !c.is_ascii_alphanumeric())
                                .filter(|w| w.len() >= 2)
                                .collect();
                            let Some(&first) = words.first() else {
                                continue;
                            };
                            if Self::is_cli_verification_clause(&words) {
                                return true;
                            }
                            if matches!(first, "start" | "launch") && words.get(1) == Some(&"dev") {
                                return true;
                            }
                            if matches!(
                                first,
                                "verify" | "validate" | "audit" | "typecheck" | "lint" | "probe"
                            ) {
                                // Ignore bare 1-word UI/pipeline stage nouns after a comma
                                // (e.g. `"Swarm: interactive DAG with parallel agents, verify, merge"`).
                                if comma_idx > 0
                                    && words.len() == 1
                                    && matches!(first, "verify" | "validate" | "audit" | "probe")
                                {
                                    continue;
                                }
                                if in_component_colon_rhs
                                    && comma_idx > 0
                                    && first != "typecheck"
                                    && !clause_has_verify_target(&words)
                                {
                                    continue;
                                }
                                return true;
                            }
                        }
                    }
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

    /// Returns true when a step explicitly requires visual/DOM browser inspection, screenshots,
    /// or viewport checks (as opposed to merely booting a dev server / probing localhost HTTP).
    pub fn step_requires_visual_browser_qa(raw_title: &str) -> bool {
        if Self::step_mentions_multi_viewport_qa(raw_title) {
            return true;
        }
        let lower = Self::strip_parentheticals_lower(raw_title);
        if lower.contains("browser_") {
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
                    | "desktop"
                    | "audit"
            )
        })
    }

    /// Returns true when `title` is an intermediate build/lint/server-boot smoke substep inside
    /// an implementation macro step (declaring no file deliverables, not a defect-remediation fix step,
    /// and not visual browser/viewport QA) that can be closed on cross-macro transition when a later
    /// macro step enforces full runtime/browser verification.
    pub(crate) fn is_deferrable_intermediate_smoke_substep(title: &str) -> bool {
        !Self::task_declares_verifiable_deliverables(title)
            && !Self::is_remediation_fix_step(title)
            && !Self::step_requires_visual_browser_qa(title)
            && (Self::step_requires_static_build_or_lint(title)
                || Self::step_requires_browser_verification(title))
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
        Self::build_verification_stage_substeps(raw_title)
    }

    fn build_verification_stage_substeps(raw_title: &str) -> Vec<String> {
        if Self::step_mentions_multi_viewport_qa(raw_title) {
            vec![
                "Run static build, typecheck, and lint verification".to_string(),
                "Start dev server and verify desktop browser layout & QA".to_string(),
                "Verify responsive tablet & mobile viewports and resolve layout issues".to_string(),
                "Run final regression build, lint, and pre-completion verification".to_string(),
            ]
        } else {
            vec![
                "Run static build, typecheck, and lint verification".to_string(),
                "Start dev server and verify live browser layout & QA".to_string(),
                "Run final regression build, lint, and pre-completion verification".to_string(),
            ]
        }
    }

    /// Extracts concrete file-path and compound symbol/component artifacts declared in `span`
    /// using structural path and identifier morphology rather than English verb allowlists.
    pub(crate) fn extract_clause_code_artifacts(span: &str) -> Vec<String> {
        let mut artifacts = Self::extract_candidate_files(span);
        for raw_tok in span.split_whitespace() {
            let clean_tok =
                raw_tok.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.');
            if Self::has_compound_identifier_morphology(clean_tok)
                && !artifacts.iter().any(|existing| existing == clean_tok)
            {
                artifacts.push(clean_tok.to_string());
            }
        }
        artifacts
    }

    /// Decomposes a hybrid assembly + verification final macro step (e.g.,
    /// `"Assembly + verification: Wire all sections into HomePage, run tsc + eslint + vite build, start dev server, screenshot 1440/768/390..."`
    /// or `"Compose all sections in src/pages/HomePage.tsx; run bun run build and browser QA"`)
    /// into BOTH its concrete page/app assembly substep AND the staged verification substeps so
    /// neither the assembly deliverable nor the verification stages are dropped.
    pub fn synthesize_hybrid_assembly_verification_substeps(raw_title: &str) -> Vec<String> {
        if !Self::step_requires_static_build_or_lint(raw_title)
            || !Self::step_requires_browser_verification(raw_title)
        {
            return Vec::new();
        }
        let clean = Self::sanitize_step_title(raw_title);
        let (_lhs_opt, body) = Self::split_once_top_level(&clean, ":")
            .or_else(|| Self::split_once_top_level(&clean, " — "))
            .map(|(l, r)| (Some(l), r))
            .unwrap_or((None, clean.as_str()));

        // Extract leading deliverable clause before `;` or `, ` (outside parens)
        let first_semi = Self::split_top_level_delimited(body, ";")
            .into_iter()
            .next()
            .unwrap_or_default();
        let candidate_clause = Self::split_top_level_delimited(&first_semi, ", ")
            .into_iter()
            .next()
            .unwrap_or(first_semi);
        let trimmed = candidate_clause.trim().trim_end_matches('.');
        if trimmed.is_empty()
            || Self::step_requires_static_build_or_lint(trimmed)
            || Self::step_requires_browser_verification(trimmed)
            || Self::clause_starts_with_operational_verb(&Self::expand_camelcase_to_lower(trimmed))
        {
            return Vec::new();
        }

        let has_concrete_artifact = !Self::extract_clause_code_artifacts(trimmed).is_empty()
            || (trimmed.split_whitespace().count() >= 2
                && Self::task_declares_verifiable_deliverables(trimmed));
        if !has_concrete_artifact {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(5);
        out.push(Self::capitalize_first_alpha(trimmed));
        out.extend(Self::build_verification_stage_substeps(raw_title));
        out
    }

    /// Splits `input` on `sep` only outside nested parentheses `()`, brackets `[]`, and braces `{}`.
    fn split_top_level_delimited(input: &str, sep: &str) -> Vec<String> {
        if sep.is_empty() {
            return vec![input.trim().to_string()];
        }
        let mut out = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        let bytes = input.as_bytes();
        let sep_bytes = sep.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    i += 1;
                }
                b')' | b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    i += 1;
                }
                _ if depth == 0
                    && i + sep_bytes.len() <= bytes.len()
                    && &bytes[i..i + sep_bytes.len()] == sep_bytes =>
                {
                    let seg = input[start..i].trim().trim_end_matches('.').trim();
                    if !seg.is_empty() {
                        out.push(seg.to_string());
                    }
                    i += sep_bytes.len();
                    start = i;
                }
                _ => {
                    let ch_len = input[i..].chars().next().map_or(1, char::len_utf8);
                    i += ch_len;
                }
            }
        }
        let tail = input[start..].trim().trim_end_matches('.').trim();
        if !tail.is_empty() {
            out.push(tail.to_string());
        }
        out
    }

    /// Splits `input` at the first occurrence of `sep` outside parentheses/brackets/braces.
    fn split_once_top_level<'a>(input: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
        let mut depth = 0usize;
        let bytes = input.as_bytes();
        let sep_bytes = sep.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    i += 1;
                }
                b')' | b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    i += 1;
                }
                _ if depth == 0
                    && i + sep_bytes.len() <= bytes.len()
                    && &bytes[i..i + sep_bytes.len()] == sep_bytes =>
                {
                    return Some((input[..i].trim(), input[i + sep.len()..].trim()));
                }
                _ => {
                    let ch_len = input[i..].chars().next().map_or(1, char::len_utf8);
                    i += ch_len;
                }
            }
        }
        None
    }

    fn capitalize_first_alpha(text: &str) -> String {
        let trimmed = text.trim();
        let mut chars = trimmed.chars();
        let Some(first) = chars.next() else {
            return String::new();
        };
        if first.is_ascii_lowercase() {
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        } else {
            trimmed.to_string()
        }
    }

    /// Automatically decomposes a multi-deliverable feature macro step into crisp Tier-2 tactical
    /// sub-steps using structural clause AST projection and code-artifact cardinality rather than
    /// hardcoded English build-verb or phase-label allowlists.
    pub fn synthesize_compound_deliverable_substeps(raw_title: &str) -> Vec<String> {
        let clean = Self::sanitize_step_title(raw_title);
        if clean.is_empty()
            || Self::is_verification_gate_step(&clean)
            || Self::step_requires_runtime_verification(&clean)
            || Self::is_remediation_fix_step(&clean)
        {
            return Vec::new();
        }

        let (lhs_opt, body) = Self::split_once_top_level(&clean, ":")
            .or_else(|| Self::split_once_top_level(&clean, " — "))
            .map(|(l, r)| (Some(l), r))
            .unwrap_or((None, clean.as_str()));

        // Parse top-level header deliverable enumeration (`" + "` or `", "` outside parens)
        let raw_lhs_items = if let Some(lhs) = lhs_opt {
            let plus = Self::split_top_level_delimited(lhs, " + ");
            if plus.len() >= 2 {
                plus
            } else {
                let comma = Self::split_top_level_delimited(lhs, ", ");
                if comma.len() >= 2 && !lhs.contains(" vs ") && !lhs.contains(" with ") {
                    comma
                } else {
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let valid_lhs_items = if raw_lhs_items.len() >= 2
            && raw_lhs_items.iter().all(|item| {
                item.split(|c: char| !c.is_ascii_alphanumeric())
                    .any(|w| w.len() >= 3)
                    && !Self::clause_starts_with_operational_verb(&Self::expand_camelcase_to_lower(
                        item,
                    ))
            }) {
            raw_lhs_items
        } else {
            Vec::new()
        };

        // Projection A: Semicolon-delimited deliverable statement list in `body`
        let semi_clauses = Self::split_top_level_delimited(body, ";");
        if (2..=8).contains(&semi_clauses.len())
            && (valid_lhs_items.len() >= 2 || semi_clauses.len() >= 3)
            && semi_clauses.iter().all(|c| {
                c.len() >= 6
                    && !Self::is_verification_gate_step(c)
                    && !Self::step_requires_runtime_verification(c)
                    && (!valid_lhs_items.is_empty()
                        || !Self::clause_starts_with_operational_verb(
                            &Self::expand_camelcase_to_lower(c),
                        )
                        || !Self::extract_clause_code_artifacts(c).is_empty()
                        || Self::task_declares_verifiable_deliverables(c))
            })
        {
            let mut out = Vec::with_capacity(semi_clauses.len());
            for (i, clause) in semi_clauses.iter().enumerate() {
                let cap_clause = Self::capitalize_first_alpha(clause);
                if valid_lhs_items.len() >= semi_clauses.len() {
                    let label = if i + 1 < semi_clauses.len() {
                        valid_lhs_items[i].clone()
                    } else {
                        valid_lhs_items[i..].join(" + ")
                    };
                    let cap_label = Self::capitalize_first_alpha(&label);
                    let clause_lower = clause.to_ascii_lowercase();
                    let label_lower = label.to_ascii_lowercase();
                    if !clause.contains(':')
                        && !clause_lower.starts_with(&format!("{label_lower} "))
                        && clause_lower != label_lower
                    {
                        out.push(format!("{}: {}", cap_label, cap_clause));
                    } else {
                        out.push(cap_clause);
                    }
                } else {
                    out.push(cap_clause);
                }
            }
            return out;
        }

        // Projection B: Period-delimited concrete code-artifact clauses when the header is a
        // single category label (`valid_lhs_items.len() < 2`), filtering prerequisite CLI/package
        // clauses that declare zero file deliverables.
        if valid_lhs_items.len() < 2 {
            let sentence_source = Self::split_once_top_level(body, " — ")
                .map(|(_, r)| r)
                .unwrap_or(body);
            let raw_period_sentences = Self::split_top_level_delimited(sentence_source, ". ");
            if (3..=8).contains(&raw_period_sentences.len()) {
                let artifact_sentences: Vec<String> = raw_period_sentences
                    .into_iter()
                    .enumerate()
                    .map(|(idx, s)| {
                        if idx == 0 {
                            let mut cur = s.as_str();
                            while let Some((sub_hdr, rhs_sent)) =
                                Self::split_once_top_level(cur, ":")
                            {
                                if Self::extract_clause_code_artifacts(sub_hdr).is_empty() {
                                    cur = rhs_sent.trim();
                                } else {
                                    break;
                                }
                            }
                            return cur.to_string();
                        }
                        s
                    })
                    .filter(|sent| {
                        !(Self::clause_starts_with_operational_verb(
                            &Self::expand_camelcase_to_lower(sent),
                        ) && Self::extract_candidate_files(sent).is_empty())
                    })
                    .collect();

                if (3..=7).contains(&artifact_sentences.len())
                    && artifact_sentences.iter().all(|sent| {
                        !sent.contains(':')
                            && !Self::clause_starts_with_operational_verb(
                                &Self::expand_camelcase_to_lower(sent),
                            )
                            && !Self::is_verification_gate_step(sent)
                            && !Self::step_requires_runtime_verification(sent)
                            && !Self::extract_clause_code_artifacts(sent).is_empty()
                    })
                {
                    return artifact_sentences
                        .into_iter()
                        .map(|s| Self::capitalize_first_alpha(&s))
                        .collect();
                }
            }
        }

        // Projection C: Header or standalone `+` / `, ` deliverable enumeration aligned with
        // `body` file/symbol stems via morphological root matching (`stem_token_matches_task`).
        let plus_source = lhs_opt.unwrap_or(body);
        let plus_items = Self::split_top_level_delimited(plus_source, " + ");
        let body_period_clauses = Self::split_top_level_delimited(body, ". ");
        let plus_items_usable = ((3..=8).contains(&plus_items.len())
            || (lhs_opt.is_some()
                && plus_items.len() == 2
                && (body_period_clauses.len() >= 2
                    || Self::extract_candidate_files(body).len() >= 2)))
            && plus_items.iter().all(|item| {
                (lhs_opt.is_some() || (!item.contains(',') && !item.contains(';')))
                    && item
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .any(|w| w.len() >= 3)
                    && !Self::clause_starts_with_operational_verb(&Self::expand_camelcase_to_lower(
                        item,
                    ))
            });
        let candidate_items: Vec<String> = if plus_items_usable {
            plus_items
        } else if (3..=8).contains(&valid_lhs_items.len()) {
            valid_lhs_items
        } else {
            Vec::new()
        };
        if !candidate_items.is_empty() {
            let rhs_identifiers = if lhs_opt.is_some() {
                Self::extract_clause_code_artifacts(body)
            } else {
                Vec::new()
            };

            let mut out = Vec::with_capacity(candidate_items.len());
            for item in &candidate_items {
                let cap_item = Self::capitalize_first_alpha(item);
                if !Self::extract_candidate_files(item).is_empty() {
                    let item_words: Vec<&str> = item.split_whitespace().collect();
                    if item_words.len() >= 2
                        && Self::extract_clause_code_artifacts(item_words[0]).is_empty()
                    {
                        out.push(cap_item);
                    } else {
                        out.push(format!("Build {}", item.trim()));
                    }
                    continue;
                }
                let item_expanded = Self::expand_camelcase_to_lower(item);
                let matched_idents: Vec<&str> = rhs_identifiers
                    .iter()
                    .map(String::as_str)
                    .filter(|ident| {
                        let stems = Self::split_identifier_stem_tokens(ident);
                        !stems.is_empty()
                            && stems
                                .iter()
                                .any(|tok| Self::stem_token_matches_task(tok, &item_expanded))
                    })
                    .collect();
                if !matched_idents.is_empty() {
                    out.push(format!("{}: Build {}", cap_item, matched_idents.join(", ")));
                } else {
                    out.push(format!("{}: Build {} section", cap_item, item.trim()));
                }
            }
            return out;
        }

        Vec::new()
    }

    /// Returns true if `tool_name` + `tool_args` switches the browser to a tablet or mobile viewport
    /// (`width <= 900` or preset `mobile`/`mobile_small`/`mobile_large`/`iphone`/`tablet`/`ipad`), either directly
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
                    "mobile"
                        | "mobile_small"
                        | "iphone_se"
                        | "mobile_large"
                        | "iphone"
                        | "tablet"
                        | "ipad"
                        | "375"
                        | "390"
                        | "414"
                        | "768"
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
        } else if Self::step_requires_visual_browser_qa(title) {
            ev.browser_desktop_verified || ev.browser_responsive_verified
        } else if Self::step_requires_browser_verification(title) {
            ev.dev_server_boot_verified
                || ev.browser_desktop_verified
                || ev.browser_responsive_verified
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

    /// Detects compiler, typecheck, linter, bundler, or test-runner error diagnostics in command
    /// stdout/stderr even when a trailing shell pipe (`bun run build 2>&1 | tail -30`) masked the
    /// non-zero exit code without `pipefail`.
    pub fn output_indicates_build_or_lint_failure(output: &str) -> bool {
        for raw_line in output.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            // TypeScript (`error TS6133:`) and Rust (`error[E0425]:`) compiler diagnostics
            if line.contains(": error TS")
                || line.starts_with("error TS")
                || line.starts_with("error[E")
            {
                return true;
            }
            // Explicit exit-code markers (`EXIT=1`, `EXIT_CODE=2`)
            for prefix in ["EXIT=", "EXIT_CODE=", "exit_code="] {
                if let Some(idx) = line.find(prefix) {
                    let rest = &line[idx + prefix.len()..];
                    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(code) = digits.parse::<i32>() {
                        if code != 0 {
                            return true;
                        }
                    }
                }
            }
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("error: could not compile")
                || lower.starts_with("error: build failed")
                || lower.contains("error during build:")
                || lower.contains("failed to compile")
                || lower.contains("command failed with exit code")
                || (lower.starts_with("error: script ") && lower.contains(" exited with code "))
                || lower.starts_with("npm err!")
                || lower.contains("elifecycle")
                || lower.contains("test result: failed.")
                || line.starts_with("FAIL ")
            {
                return true;
            }
            if (line.contains('✖') || lower.contains("problems ("))
                && lower.contains("error")
                && !lower.contains("0 error")
            {
                return true;
            }
        }
        false
    }

    /// Evaluates whether a tool execution succeeded for verification/transition purposes, rejecting
    /// piped static build/lint/test commands whose output contains compiler, linter, or test errors.
    pub fn is_effective_tool_verification_success(
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_success: bool,
        tool_output: &str,
    ) -> bool {
        if !tool_success {
            return false;
        }
        // Query structured runtime state in `MiniDevRegistry` (`is_command_currently_detached`)
        // instead of matching formatted display prefixes: commands still executing asynchronously
        // in `minitask` record verification evidence only after `harvest_newly_completed_tasks_block`
        // observes exit code `0`.
        if matches!(tool_name, "exec_cmd" | "bash" | "shell" | "run_command") {
            if let Some(cmd) = tool_args
                .get("command")
                .or_else(|| tool_args.get("cmd"))
                .and_then(|v| v.as_str())
            {
                if crate::dev::registry::get_global_dev_registry()
                    .is_command_currently_detached(cmd)
                {
                    return false;
                }
            }
        }
        if Self::tool_runs_static_build_or_lint(tool_name, tool_args)
            && Self::output_indicates_build_or_lint_failure(tool_output)
        {
            return false;
        }
        true
    }

    /// Output-aware wrapper around `record_verification_tool_activity` that guards against piped
    /// build/lint/test commands exiting `0` while printing compiler or linter errors.
    pub fn record_verification_tool_activity_with_output(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_success: bool,
        tool_output: &str,
    ) {
        let effective_ok = Self::is_effective_tool_verification_success(
            tool_name,
            tool_args,
            tool_success,
            tool_output,
        );
        self.record_verification_tool_activity(tool_name, tool_args, effective_ok);
    }

    /// Output-aware wrapper around `reconcile_verification_substep_on_tool_transition` that guards
    /// against piped build/lint/test commands exiting `0` while printing compiler or linter errors.
    pub fn reconcile_verification_substep_on_tool_output(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_success: bool,
        tool_output: &str,
    ) -> usize {
        let effective_ok = Self::is_effective_tool_verification_success(
            tool_name,
            tool_args,
            tool_success,
            tool_output,
        );
        self.reconcile_verification_substep_on_tool_transition(tool_name, tool_args, effective_ok)
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
            if !ev.dev_server_boot_verified {
                ev.dev_server_boot_verified = true;
                changed = true;
            }
            if ev.dirty_since_browser_qa {
                ev.dirty_since_browser_qa = false;
                changed = true;
            }
            if ev.responsive_mode_active {
                if !ev.browser_responsive_verified {
                    ev.browser_responsive_verified = true;
                    changed = true;
                }
            } else if !ev.browser_desktop_verified {
                ev.browser_desktop_verified = true;
                changed = true;
            }
        } else if !ev.dev_server_boot_verified {
            let boots_or_probes_dev_server = match tool_name {
                "browser_navigate" => true,
                "minitask" => tool_args
                    .get("action")
                    .and_then(|v| v.as_str())
                    .is_some_and(|act| {
                        matches!(
                            act.to_ascii_lowercase().as_str(),
                            "start" | "restart" | "logs" | "status" | "list"
                        )
                    }),
                "exec_cmd" | "bash" | "shell" | "run_command" => tool_args
                    .get("command")
                    .or_else(|| tool_args.get("cmd"))
                    .and_then(|v| v.as_str())
                    .is_some_and(|cmd| {
                        let lower = cmd.to_ascii_lowercase();
                        lower.contains("localhost")
                            || lower.contains("127.0.0.1")
                            || lower.contains("http://")
                            || crate::tools::exec::CommandProfile::analyze(cmd).workload
                                == crate::tools::exec::CommandWorkload::ServerDaemon
                    }),
                _ => false,
            };
            if boots_or_probes_dev_server {
                ev.dev_server_boot_verified = true;
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

    /// Returns the first workspace-relative application code or config path in `modified_files`
    /// (ignoring internal `.minicode/`, docs, screenshots, markdown, images, and lockfiles).
    fn first_code_or_config_mutation(modified_files: &[String]) -> Option<String> {
        modified_files.iter().find_map(|raw| {
            let rel = raw.trim_start_matches("./");
            if rel.is_empty()
                || rel.starts_with(".minicode/")
                || rel.contains("/.minicode/")
                || rel.starts_with("minikit_docs/")
                || rel.starts_with("onpkg_docs/")
                || rel.starts_with("screenshots/")
            {
                return None;
            }
            let ext = Path::new(rel)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(
                ext.as_str(),
                "md" | "txt" | "png" | "jpg" | "jpeg" | "webp" | "gif" | "ico" | "lock"
            ) {
                None
            } else {
                Some(rel.to_string())
            }
        })
    }

    /// Removes stale `.minicode/sdd/*/review_package.md` and `review-*.diff` artifacts after source
    /// code mutations so resolved `power_review` findings are never folded into `archive_plan` as unresolved defects.
    fn clear_stale_sdd_review_packages(&self) {
        let sdd_root = self.workspace_root.join(".minicode").join("sdd");
        let Ok(entries) = fs::read_dir(&sdd_root) else {
            return;
        };
        for entry in entries.flatten() {
            let sub = entry.path();
            if sub.is_dir() {
                if let Ok(files) = fs::read_dir(&sub) {
                    for file_entry in files.flatten() {
                        let fp = file_entry.path();
                        if !fp.is_file() {
                            continue;
                        }
                        let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        let is_review_diff = fp
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("diff"));
                        if fname == "review_package.md" || is_review_diff {
                            let _ = fs::remove_file(&fp);
                        }
                    }
                }
                if fs::read_dir(&sub)
                    .map(|mut it| it.next().is_none())
                    .unwrap_or(false)
                {
                    let _ = fs::remove_dir(&sub);
                }
            }
        }
        if fs::read_dir(&sdd_root)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false)
        {
            let _ = fs::remove_dir(&sdd_root);
        }
    }

    /// Invalidates stale static-build verification evidence (and marks browser QA dirty until the
    /// next visual/DOM check) whenever application source or config files are mutated on disk.
    pub fn invalidate_verification_evidence_on_code_mutation(&self, modified_files: &[String]) {
        if !self.has_active_plan() || modified_files.is_empty() {
            return;
        }
        if Self::first_code_or_config_mutation(modified_files).is_none() {
            return;
        }
        self.clear_stale_sdd_review_packages();
        let mut ev = self.load_verification_evidence();
        let mut changed = false;
        if ev.static_build_verified {
            ev.static_build_verified = false;
            changed = true;
        }
        if (ev.browser_desktop_verified || ev.browser_responsive_verified)
            && !ev.dirty_since_browser_qa
        {
            ev.dirty_since_browser_qa = true;
            changed = true;
        }
        if changed {
            self.save_verification_evidence(&ev);
        }
    }

    /// Reopens the final verification/completion step back to `[>]` (`InProgress`) when source or
    /// config files are modified while an active unarchived plan is already at 100% `[x]` completion,
    /// preventing the TUI todo and `progress.md` from staying frozen at `N/N (active=None)` across
    /// late post-audit bugfixes and re-verification runs.
    pub fn reopen_completed_plan_on_code_mutation(&self, modified_files: &[String]) -> bool {
        if !self.has_active_plan() || modified_files.is_empty() {
            return false;
        }
        let Some(first_mutated) = Self::first_code_or_config_mutation(modified_files) else {
            return false;
        };
        if !self.is_all_project_tasks_completed() {
            return false;
        }

        let reopen_marker = |line: &str| -> String {
            if line.contains("[x]") {
                line.replacen("[x]", "[>]", 1)
            } else if line.contains("[X]") {
                line.replacen("[X]", "[>]", 1)
            } else {
                line.to_string()
            }
        };

        const APPENDED_VERIFY_TITLE: &str =
            "Verify build, runtime behavior, and modified workspace deliverables";

        let is_verification_or_remediation = |title: &str| -> bool {
            Self::step_requires_runtime_verification(title) || Self::is_remediation_fix_step(title)
        };

        let mut reopened_title: Option<String> = None;
        let mut reopened_subplan: Option<(String, Vec<(TaskItemStatus, String)>)> = None;
        let mut macro_plan_override: Option<(String, Vec<(TaskItemStatus, String)>)> = None;

        let todo_path = self.canonical_todo_path();
        if todo_path.is_file() {
            if let Ok(todo_raw) = fs::read_to_string(&todo_path) {
                let mut lines: Vec<String> = todo_raw.lines().map(str::to_string).collect();
                if let Some(last_top_idx) = lines.iter().rposition(|l| {
                    !Self::is_indented_subtask_line(l)
                        && (l.contains("[x]") || l.contains("[X]"))
                        && Self::parse_task_line(0, l.trim()).is_some()
                }) {
                    let parent_title = Self::parse_task_line(0, lines[last_top_idx].trim())
                        .map(|t| t.title)
                        .unwrap_or_default();

                    let mut sub_end = last_top_idx + 1;
                    while sub_end < lines.len() && Self::is_indented_subtask_line(&lines[sub_end]) {
                        sub_end += 1;
                    }

                    let parent_is_verify = is_verification_or_remediation(&parent_title);
                    let verify_sub_rel_idx = if sub_end > last_top_idx + 1 {
                        lines[(last_top_idx + 1)..sub_end].iter().rposition(|l| {
                            (l.contains("[x]") || l.contains("[X]"))
                                && Self::parse_task_line(0, l.trim())
                                    .is_some_and(|p| is_verification_or_remediation(&p.title))
                        })
                    } else {
                        None
                    };

                    if parent_is_verify || verify_sub_rel_idx.is_some() {
                        lines[last_top_idx] = reopen_marker(&lines[last_top_idx]);
                        if sub_end > last_top_idx + 1 {
                            let target_rel = verify_sub_rel_idx.or_else(|| {
                                lines[(last_top_idx + 1)..sub_end]
                                    .iter()
                                    .rposition(|l| l.contains("[x]") || l.contains("[X]"))
                            });
                            if let Some(rel_last_sub) = target_rel {
                                let abs_sub_idx = last_top_idx + 1 + rel_last_sub;
                                lines[abs_sub_idx] = reopen_marker(&lines[abs_sub_idx]);
                                let mut sub_items = Vec::new();
                                for sub_line in &lines[(last_top_idx + 1)..sub_end] {
                                    if let Some(parsed) = Self::parse_task_line(0, sub_line.trim())
                                    {
                                        if parsed.status == TaskItemStatus::InProgress {
                                            reopened_title = Some(parsed.title.clone());
                                        }
                                        sub_items.push((parsed.status, parsed.title));
                                    }
                                }
                                if !parent_title.is_empty() && !sub_items.is_empty() {
                                    reopened_subplan = Some((parent_title, sub_items));
                                }
                            }
                        } else if !parent_title.is_empty() {
                            reopened_title = Some(parent_title);
                        }
                    } else {
                        // Plan reached 100% without any verification gate step: keep completed
                        // implementation/scaffolding tasks `[x]` and append an explicit `[>]`
                        // verification gate rather than falsely reopening a finished build step.
                        lines.insert(sub_end, format!("- [>] {}", APPENDED_VERIFY_TITLE));
                        reopened_title = Some(APPENDED_VERIFY_TITLE.to_string());
                        let root_title = self
                            .read_roadmap_root_title()
                            .filter(|r| !r.is_empty())
                            .unwrap_or_else(|| "Task Plan".to_string());
                        let mut macro_items = Vec::new();
                        for line in &lines {
                            if !Self::is_indented_subtask_line(line) {
                                if let Some(parsed) = Self::parse_task_line(0, line.trim()) {
                                    macro_items.push((parsed.status, parsed.title));
                                }
                            }
                        }
                        if !macro_items.is_empty() {
                            macro_plan_override = Some((root_title, macro_items));
                        }
                    }
                    let _ = fs::write(&todo_path, format!("{}\n", lines.join("\n")));
                }
            }
        }

        let timestamp = chrono::Utc::now()
            .format(crate::constants::TIMESTAMP_FORMAT)
            .to_string();

        if let Some((parent_title, sub_items)) = reopened_subplan {
            let root_goal_line = self
                .read_roadmap_root_title()
                .filter(|root| !root.is_empty())
                .map(|root| format!("> Root Goal: {}\n", root))
                .unwrap_or_default();
            let mut plan_content = format!(
                "# Task Plan: {}\n\n> Created: {}\n> Mode: subplan\n{}\n## Objectives & Steps:\n",
                parent_title, timestamp, root_goal_line
            );
            for (idx, (st, sub_title)) in sub_items.iter().enumerate() {
                let marker = match st {
                    TaskItemStatus::Completed => "[x]",
                    TaskItemStatus::InProgress => "[>]",
                    TaskItemStatus::Pending => "[ ]",
                };
                plan_content.push_str(&format!("{}. {} {}\n", idx + 1, marker, sub_title));
            }
            let _ = fs::write(self.task_plan_path(), plan_content);
        } else if let Some((root_title, macro_items)) = macro_plan_override {
            let mut plan_content = format!(
                "# Task Plan: {}\n\n> Created: {}\n\n## Objectives & Steps:\n",
                root_title, timestamp
            );
            for (idx, (st, step_title)) in macro_items.iter().enumerate() {
                let marker = match st {
                    TaskItemStatus::Completed => "[x]",
                    TaskItemStatus::InProgress => "[>]",
                    TaskItemStatus::Pending => "[ ]",
                };
                plan_content.push_str(&format!("{}. {} {}\n", idx + 1, marker, step_title));
            }
            let _ = fs::write(self.task_plan_path(), plan_content);
        } else if let Ok(plan_raw) = fs::read_to_string(self.task_plan_path()) {
            let mut lines: Vec<String> = plan_raw.lines().map(str::to_string).collect();
            if let Some(last_idx) = lines.iter().rposition(|l| {
                (l.contains("[x]") || l.contains("[X]"))
                    && Self::parse_task_line(0, l.trim()).is_some()
            }) {
                let last_task_title = Self::parse_task_line(0, lines[last_idx].trim())
                    .map(|t| t.title)
                    .unwrap_or_default();
                if is_verification_or_remediation(&last_task_title) {
                    if reopened_title.is_none() {
                        reopened_title = Some(last_task_title);
                    }
                    lines[last_idx] = reopen_marker(&lines[last_idx]);
                } else {
                    let count = lines
                        .iter()
                        .filter(|l| Self::parse_task_line(0, l.trim()).is_some())
                        .count();
                    lines.insert(
                        last_idx + 1,
                        format!("{}. [>] {}", count + 1, APPENDED_VERIFY_TITLE),
                    );
                    if reopened_title.is_none() {
                        reopened_title = Some(APPENDED_VERIFY_TITLE.to_string());
                    }
                }
                let _ = fs::write(self.task_plan_path(), format!("{}\n", lines.join("\n")));
            }
        }

        let Some(step_label) = reopened_title else {
            return false;
        };

        let p_path = self.progress_path();
        if let Ok(prog_raw) = fs::read_to_string(&p_path) {
            let mut updated_prog =
                prog_raw.replacen("- Status: Completed (100%)", "- Status: In Progress", 1);
            updated_prog.push_str(&format!(
                "\n- [{}] **Reopened for Re-Verification**: [>] {} (source modified: `{}`)\n",
                timestamp, step_label, first_mutated
            ));
            let _ = fs::write(&p_path, updated_prog);
        }

        let impl_path = self.canonical_implementation_path();
        if impl_path.is_file() {
            if let Ok(impl_raw) = fs::read_to_string(&impl_path) {
                if impl_raw.contains("status: completed") {
                    let _ = fs::write(
                        &impl_path,
                        impl_raw.replacen("status: completed", "status: active", 1),
                    );
                }
            }
        }

        true
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

        // Immediate scaffold substep / step completion when `kit_stack_add` succeeds and the
        // project manifest is present on disk (e.g. `"Run kit_stack_add stack_name=react-vite"`).
        if matches!(
            tool_name,
            "kit_stack_add" | "minikit_stack_add" | "onpkg_stack_add"
        ) && crate::agent::orchestrator::WorkflowRouter::project_manifest(&self.workspace_root)
            .is_some()
        {
            let tasks = self.read_parsed_tasks();
            if let Some(active_idx) = tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
            {
                let active_title = &tasks[active_idx].title;
                let lower = active_title.to_ascii_lowercase();
                let is_pure_scaffold_step = Self::extract_candidate_files(active_title).is_empty()
                    && !Self::step_requires_runtime_verification(active_title)
                    && (lower.contains("kit_stack_add")
                        || lower.contains("onpkg")
                        || (active_idx == 0
                            && !Self::task_declares_verifiable_deliverables(active_title)
                            && self.is_step_deliverables_satisfied_on_disk(active_idx, &tasks)));
                if is_pure_scaffold_step && self.update_progress(active_title, "completed").is_ok()
                {
                    reconciled += 1;
                    self.record_auto_promoted_active_step();
                    return reconciled;
                }
            }
        }

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
        // Stage 0 (Tactical Subplan -> Verification Milestone): If the active macro step is a
        // non-verification deliverable step in `Mode: subplan` whose deliverables are 100% on disk
        // and the agent begins running build/lint/browser commands for a subsequent verification
        // macro step, auto-close remaining non-verification (or intermediate static smoke) substeps
        // across consecutive completed macro steps until reaching the active verification milestone.
        if self.is_tactical_subplan()
            && (starts_browser_stage || Self::tool_runs_static_build_or_lint(tool_name, tool_args))
        {
            for _ in 0..8 {
                if !self.is_tactical_subplan() {
                    break;
                }
                let milestones = self.read_roadmap_milestones();
                let Some(m) = milestones.iter().find(|m| m.is_active) else {
                    break;
                };
                let Some(parent_idx) = m
                    .tasks
                    .iter()
                    .position(|t| t.status == TaskItemStatus::InProgress)
                else {
                    break;
                };
                let parent_title = &m.tasks[parent_idx].title;
                let later_has_verification = m.tasks[(parent_idx + 1)..].iter().any(|t| {
                    t.status != TaskItemStatus::Completed
                        && Self::step_requires_runtime_verification(&t.title)
                });
                if Self::step_requires_runtime_verification(parent_title) || !later_has_verification
                {
                    break;
                }
                let sub_tasks = self.read_task_plan_tasks();
                let remaining_indices: Vec<usize> = (0..sub_tasks.len())
                    .filter(|&i| sub_tasks[i].status != TaskItemStatus::Completed)
                    .collect();
                if remaining_indices.is_empty() {
                    break;
                }
                let subs_allow_close = remaining_indices.iter().all(|&i| {
                    let t = &sub_tasks[i].title;
                    !Self::step_requires_runtime_verification(t)
                        || Self::is_deferrable_intermediate_smoke_substep(t)
                });
                let deliverables_ready =
                    (Self::task_declares_verifiable_deliverables(parent_title)
                        && self.are_task_deliverables_verified_on_disk(parent_idx, &m.tasks))
                        || (self.is_step_deliverables_satisfied_on_disk(parent_idx, &m.tasks)
                            && remaining_indices.iter().all(|&i| {
                                Self::is_deferrable_intermediate_smoke_substep(&sub_tasks[i].title)
                                    || self.is_step_deliverables_satisfied_on_disk(i, &sub_tasks)
                            }));
                if !subs_allow_close || !deliverables_ready {
                    break;
                }
                let mut closed_in_pass = 0usize;
                for i in remaining_indices {
                    if self
                        .update_progress(&sub_tasks[i].title, "completed")
                        .is_ok()
                    {
                        closed_in_pass += 1;
                    }
                }
                if closed_in_pass == 0 {
                    break;
                }
                reconciled += closed_in_pass;
            }
            if reconciled > 0 {
                self.record_auto_promoted_active_step();
                return reconciled;
            }
        }

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

        // Stage 0.5 -> Stage 1/2: Hybrid assembly/deliverable substep -> Static build or Browser QA substep
        if starts_browser_stage || Self::tool_runs_static_build_or_lint(tool_name, tool_args) {
            let sub_tasks = self.read_task_plan_tasks();
            if let Some(active_idx) = sub_tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
            {
                if let Some(next_sub) = sub_tasks.get(active_idx + 1) {
                    let active_title = &sub_tasks[active_idx].title;
                    if !Self::step_requires_runtime_verification(active_title)
                        && Self::step_requires_runtime_verification(&next_sub.title)
                        && self.is_step_deliverables_satisfied_on_disk(active_idx, &sub_tasks)
                        && self.update_progress(active_title, "completed").is_ok()
                    {
                        reconciled += 1;
                    }
                }
            }
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

        // Stage 2/3 -> Remediation ("Fix ..."): Browser QA substep -> Defect-remediation substep
        // Fires either when the agent edits code to fix an issue discovered during browser QA,
        // or when clean closeout/regression runs with zero defects needing edits.
        let is_code_fix_mutation = matches!(
            tool_name,
            "write_file" | "patch_file" | "apply_patch" | "edit_file"
        );
        let switches_back_to_desktop = tool_name == "browser_emulate"
            && !Self::tool_switches_to_responsive_viewport(tool_name, tool_args);
        let triggers_closeout_regression =
            matches!(
                tool_name,
                "power_review" | "minipower_review" | "power_verify" | "minipower_verify"
            ) || Self::tool_runs_static_build_or_lint(tool_name, tool_args)
                || Self::tool_performs_browser_qa_check(tool_name, tool_args)
                || switches_back_to_desktop;

        if is_code_fix_mutation || triggers_closeout_regression {
            let sub_tasks = self.read_task_plan_tasks();
            if let Some(active_idx) = sub_tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
            {
                if let Some(next_sub) = sub_tasks.get(active_idx + 1) {
                    let active_title = &sub_tasks[active_idx].title;
                    if !Self::task_declares_verifiable_deliverables(active_title)
                        && Self::step_requires_browser_verification(active_title)
                        && Self::is_remediation_fix_step(&next_sub.title)
                    {
                        let ev = self.load_verification_evidence();
                        let browser_ran = if Self::step_mentions_multi_viewport_qa(active_title) {
                            ev.browser_desktop_verified && ev.browser_responsive_verified
                        } else {
                            ev.browser_desktop_verified || ev.browser_responsive_verified
                        };
                        let ready_for_remediation = is_code_fix_mutation
                            || matches!(
                                tool_name,
                                "power_review"
                                    | "minipower_review"
                                    | "power_verify"
                                    | "minipower_verify"
                            )
                            || (Self::tool_runs_static_build_or_lint(tool_name, tool_args)
                                && !ev.dirty_since_browser_qa)
                            || (switches_back_to_desktop
                                && ev.static_build_verified
                                && !ev.dirty_since_browser_qa);
                        if ev.auto_promoted_step.as_deref() != Some(active_title.as_str())
                            && browser_ran
                            && ready_for_remediation
                            && self.update_progress(active_title, "completed").is_ok()
                        {
                            reconciled += 1;
                            if is_code_fix_mutation {
                                self.record_auto_promoted_active_step();
                                return reconciled;
                            }
                        }
                    }
                }
            }
        }

        // Stage 3 -> Stage 4 (or Remediation "Fix ..." -> Final Regression / Closeout)
        if triggers_closeout_regression {
            let sub_tasks = self.read_task_plan_tasks();
            if let Some(active_idx) = sub_tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
            {
                let active_title = &sub_tasks[active_idx].title;
                if let Some(next_sub) = sub_tasks.get(active_idx + 1) {
                    if !Self::task_declares_verifiable_deliverables(active_title)
                        && Self::step_requires_browser_verification(active_title)
                        && Self::step_requires_static_build_or_lint(&next_sub.title)
                        && !Self::step_requires_browser_verification(&next_sub.title)
                    {
                        let ev = self.load_verification_evidence();
                        let closeout_ready = matches!(
                            tool_name,
                            "power_review"
                                | "minipower_review"
                                | "power_verify"
                                | "minipower_verify"
                        ) || (Self::tool_runs_static_build_or_lint(
                            tool_name, tool_args,
                        ) && !ev.dirty_since_browser_qa)
                            || (Self::tool_performs_browser_qa_check(tool_name, tool_args)
                                && ev.static_build_verified
                                && !ev.responsive_mode_active)
                            || (switches_back_to_desktop
                                && ev.static_build_verified
                                && !ev.dirty_since_browser_qa);
                        if self.is_verification_step_evidence_satisfied(active_title, &ev)
                            && closeout_ready
                            && self.update_progress(active_title, "completed").is_ok()
                        {
                            reconciled += 1;
                        }
                    } else if Self::is_remediation_fix_step(active_title)
                        && Self::step_requires_static_build_or_lint(&next_sub.title)
                    {
                        let ev = self.load_verification_evidence();
                        let post_fix_ready = matches!(
                            tool_name,
                            "power_review"
                                | "minipower_review"
                                | "power_verify"
                                | "minipower_verify"
                        ) || (Self::tool_runs_static_build_or_lint(
                            tool_name, tool_args,
                        ) && !ev.dirty_since_browser_qa);
                        if ev.auto_promoted_step.as_deref() != Some(active_title.as_str())
                            && post_fix_ready
                            && self.update_progress(active_title, "completed").is_ok()
                        {
                            reconciled += 1;
                        }
                    }
                } else if Self::is_remediation_fix_step(active_title)
                    && matches!(
                        tool_name,
                        "power_review" | "minipower_review" | "power_verify" | "minipower_verify"
                    )
                {
                    let ev = self.load_verification_evidence();
                    if !ev.dirty_since_browser_qa
                        && (ev.static_build_verified
                            || matches!(tool_name, "power_verify" | "minipower_verify"))
                        && self.update_progress(active_title, "completed").is_ok()
                    {
                        reconciled += 1;
                    }
                } else if !Self::task_declares_verifiable_deliverables(active_title)
                    && matches!(tool_name, "power_verify" | "minipower_verify")
                {
                    let ev = self.load_verification_evidence();
                    let browser_ok = !Self::step_requires_browser_verification(active_title)
                        || self.is_verification_step_evidence_satisfied(active_title, &ev);
                    if !ev.dirty_since_browser_qa
                        && browser_ok
                        && Self::step_requires_static_build_or_lint(active_title)
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

    /// Returns true when `raw_title` is a defect-remediation substep inside a verification milestone
    /// (e.g. `"Fix any geometry, contrast, or responsive issues found"`).
    pub(crate) fn is_remediation_fix_step(raw_title: &str) -> bool {
        if !Self::extract_candidate_files(raw_title).is_empty() {
            return false;
        }
        let lower = Self::strip_parentheticals_lower(raw_title);
        lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .find(|w| w.len() >= 2)
            .is_some_and(|first| {
                matches!(
                    first,
                    "fix" | "fixes" | "resolve" | "remediate" | "address" | "repair" | "polish"
                )
            })
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

    /// Maps explicit deliverable candidate files declared on the active parent macro step in `todo.md`
    /// onto its child substeps in `task_plan.md` when `tasks` represents the active tactical sub-plan.
    ///
    /// Uses a 3-pass monotonic bipartite alignment:
    /// - Pass 1 (Strict Stem Alignment): Assigns parent candidate files whose domain stems all align
    ///   with a substep (processing shorter/more specific stems first, allowing multi-clause substeps
    ///   such as `"Final CTA + colophon footer"` to claim both `CTA.tsx` and `Footer.tsx`).
    /// - Pass 1.5 (Monotonic Interval Unique Stem Match): Within each interval bounded by Pass 1
    ///   anchors, assigns an unassigned file (such as `HeroTerminal.tsx` or `CodeIntelligence.tsx`)
    ///   if it has `head_score > 0` on a unique unclaimed substep inside that interval.
    /// - Pass 2 (Monotonic Interval 1-to-1 Positional Alignment): Within each anchored interval where
    ///   the count of remaining unassigned parent files equals the count of remaining unclaimed
    ///   eligible substeps (e.g. `Interfaces.tsx` -> `"Two interaction modes"`), pairs them 1-to-1.
    fn effective_subplan_task_candidates(&self, tasks: &[TaskItem]) -> Vec<Vec<String>> {
        let mut base_cands: Vec<Vec<String>> = tasks
            .iter()
            .map(|t| Self::extract_candidate_files(&t.title))
            .collect();
        if !self.is_tactical_subplan() || tasks.is_empty() {
            return base_cands;
        }
        let sub_tasks = self.read_task_plan_tasks();
        if sub_tasks.len() != tasks.len()
            || !sub_tasks
                .iter()
                .zip(tasks.iter())
                .all(|(a, b)| a.title == b.title)
        {
            return base_cands;
        }
        let Some(parent_task) = self
            .read_roadmap_milestones()
            .into_iter()
            .find(|m| m.is_active)
            .and_then(|m| {
                m.tasks
                    .iter()
                    .find(|t| t.status == TaskItemStatus::InProgress)
                    .cloned()
                    .or_else(|| {
                        m.tasks
                            .into_iter()
                            .find(|t| t.status == TaskItemStatus::Pending)
                    })
            })
        else {
            return base_cands;
        };

        let parent_cands = Self::extract_candidate_files(&parent_task.title);
        if parent_cands.len() < 2 {
            return base_cands;
        }

        let eligible_sub: Vec<bool> = tasks
            .iter()
            .enumerate()
            .map(|(idx, t)| {
                base_cands[idx].is_empty()
                    && !Self::is_verification_gate_step(&t.title)
                    && !Self::step_requires_runtime_verification(&t.title)
                    && !Self::is_remediation_fix_step(&t.title)
                    && self.check_package_install_step_on_disk(&t.title).is_none()
            })
            .collect();

        let sub_lowers: Vec<String> = tasks
            .iter()
            .map(|t| Self::expand_camelcase_to_lower(&t.title))
            .collect();
        let sub_heads: Vec<String> = tasks
            .iter()
            .map(|t| Self::extract_primary_deliverable_head(&t.title))
            .collect();
        let sub_clauses: Vec<Vec<String>> = sub_heads
            .iter()
            .map(|h| Self::split_heading_clauses(h))
            .collect();

        let mut file_to_sub: Vec<Option<usize>> = vec![None; parent_cands.len()];
        let mut sub_claimed: Vec<bool> = base_cands.iter().map(|c| !c.is_empty()).collect();

        // Anchor any parent candidate already named verbatim in a substep's own title
        for (f_idx, pc) in parent_cands.iter().enumerate() {
            if let Some(s_idx) = base_cands
                .iter()
                .position(|cands| cands.iter().any(|c| Self::paths_match_by_components(c, pc)))
            {
                file_to_sub[f_idx] = Some(s_idx);
            }
        }

        let file_stems: Vec<Vec<String>> = parent_cands
            .iter()
            .map(|pc| Self::split_identifier_stem_tokens(pc))
            .collect();

        // Pass 1: Strict stem alignment (ordered by stem count ascending so 1-stem files like
        // `Hero.tsx` claim their substep before 2-stem siblings like `HeroTerminal.tsx`).
        let mut f_order: Vec<usize> = (0..parent_cands.len()).collect();
        f_order.sort_by_key(|&f_idx| (file_stems[f_idx].len(), f_idx));

        for f_idx in f_order {
            if file_to_sub[f_idx].is_some() || file_stems[f_idx].is_empty() {
                continue;
            }
            let stems = &file_stems[f_idx];
            let mut best_match: Option<(bool, f64, usize)> = None;
            for s_idx in 0..tasks.len() {
                if !eligible_sub[s_idx] {
                    continue;
                }
                if sub_claimed[s_idx] && sub_clauses[s_idx].len() < 2 {
                    continue;
                }
                let (head_score, total_score) =
                    Self::score_file_to_task(stems, s_idx, tasks, &sub_heads, &sub_lowers);
                if head_score <= 0.0 {
                    continue;
                }
                let clauses = &sub_clauses[s_idx];
                let strict_ok = if clauses.len() == 1 {
                    !Self::is_pure_data_helper_for_clause(&parent_cands[f_idx], &clauses[0])
                        && Self::single_clause_segments_align(
                            stems,
                            &clauses[0],
                            &sub_lowers[s_idx],
                        )
                } else {
                    !clauses.is_empty()
                        && stems
                            .iter()
                            .all(|tok| Self::stem_token_matches_task(tok, &sub_lowers[s_idx]))
                };
                if !strict_ok {
                    continue;
                }
                let is_unclaimed = !sub_claimed[s_idx];
                let replace = match best_match {
                    None => true,
                    Some((best_unclaimed, best_score, _)) => {
                        if total_score > best_score + 1e-6 {
                            true
                        } else {
                            (total_score - best_score).abs() <= 1e-6
                                && is_unclaimed
                                && !best_unclaimed
                        }
                    }
                };
                if replace {
                    best_match = Some((is_unclaimed, total_score, s_idx));
                }
            }
            if let Some((_, _, chosen_s)) = best_match {
                file_to_sub[f_idx] = Some(chosen_s);
                sub_claimed[chosen_s] = true;
            }
        }

        let build_monotonic_intervals =
            |f_map: &[Option<usize>]| -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
                let mut anchors: Vec<(usize, usize)> = Vec::new();
                for (f_i, maybe_s) in f_map.iter().enumerate() {
                    if let Some(s_i) = *maybe_s {
                        if anchors.last().is_none_or(|&(_, prev_s)| s_i >= prev_s) {
                            anchors.push((f_i, s_i));
                        }
                    }
                }
                if anchors.is_empty() {
                    return vec![(0..parent_cands.len(), 0..tasks.len())];
                }
                let mut intervals = Vec::new();
                let (first_f, first_s) = anchors[0];
                if first_f > 0 && first_s > 0 {
                    intervals.push((0..first_f, 0..first_s));
                }
                for win in anchors.windows(2) {
                    let (fa, sa) = win[0];
                    let (fb, sb) = win[1];
                    if fa + 1 < fb && sa + 1 < sb {
                        intervals.push(((fa + 1)..fb, (sa + 1)..sb));
                    }
                }
                if let Some(&(last_f, last_s)) = anchors.last() {
                    if last_f + 1 < parent_cands.len() && last_s + 1 < tasks.len() {
                        intervals
                            .push(((last_f + 1)..parent_cands.len(), (last_s + 1)..tasks.len()));
                    }
                }
                intervals
            };

        // Pass 1.5: Unique `head_score > 0` match inside monotonic anchor intervals
        for (f_range, s_range) in build_monotonic_intervals(&file_to_sub) {
            for f_idx in f_range {
                if file_to_sub[f_idx].is_some() || file_stems[f_idx].is_empty() {
                    continue;
                }
                let matching_subs: Vec<usize> = s_range
                    .clone()
                    .filter(|&s_idx| {
                        eligible_sub[s_idx]
                            && !sub_claimed[s_idx]
                            && Self::score_file_to_task(
                                &file_stems[f_idx],
                                s_idx,
                                tasks,
                                &sub_heads,
                                &sub_lowers,
                            )
                            .0 > 0.0
                    })
                    .collect();
                if matching_subs.len() == 1 {
                    let chosen_s = matching_subs[0];
                    file_to_sub[f_idx] = Some(chosen_s);
                    sub_claimed[chosen_s] = true;
                }
            }
        }

        // Pass 2: 1-to-1 positional alignment inside monotonic intervals with equal counts
        for (f_range, s_range) in build_monotonic_intervals(&file_to_sub) {
            let unassigned_f: Vec<usize> = f_range
                .filter(|&f_idx| file_to_sub[f_idx].is_none())
                .collect();
            let unassigned_s: Vec<usize> = s_range
                .filter(|&s_idx| eligible_sub[s_idx] && !sub_claimed[s_idx])
                .collect();
            if !unassigned_f.is_empty() && unassigned_f.len() == unassigned_s.len() {
                for (f_idx, s_idx) in unassigned_f.into_iter().zip(unassigned_s.into_iter()) {
                    file_to_sub[f_idx] = Some(s_idx);
                    sub_claimed[s_idx] = true;
                }
            }
        }

        for (f_idx, maybe_s) in file_to_sub.into_iter().enumerate() {
            if let Some(s_idx) = maybe_s {
                let cand = &parent_cands[f_idx];
                if !base_cands[s_idx]
                    .iter()
                    .any(|existing| Self::paths_match_by_components(existing, cand))
                {
                    base_cands[s_idx].push(cand.clone());
                }
            }
        }

        base_cands
    }

    /// Checks whether all declared deliverables for `tasks[task_idx]` are verified as non-empty,
    /// non-scaffold files on disk under global bipartite task ownership.
    /// Supports both compound-identifier deliverables (`TerminalPreview`, `SwarmVisual`) and
    /// single-word component deliverables (`Philosophy`, `Council`, `Tools`, `Interfaces`, `Footer`)
    /// once a uniquely owned non-scaffold workspace file matching the deliverable head exists on disk.
    pub(super) fn are_task_deliverables_verified_on_disk(
        &self,
        task_idx: usize,
        tasks: &[TaskItem],
    ) -> bool {
        let Some(task) = tasks.get(task_idx) else {
            return false;
        };
        let direct_candidates = Self::extract_candidate_files(&task.title);
        if !direct_candidates.is_empty() {
            return direct_candidates
                .iter()
                .all(|c| self.is_candidate_completed_on_disk(c));
        }
        let task_explicit_candidates = self.effective_subplan_task_candidates(tasks);
        if let Some(mapped) = task_explicit_candidates.get(task_idx) {
            if !mapped.is_empty() {
                return mapped
                    .iter()
                    .all(|c| self.is_candidate_completed_on_disk(c));
            }
        }
        let explicitly_declares = Self::task_declares_verifiable_deliverables(&task.title);
        if Self::is_verification_gate_step(&task.title)
            || Self::is_remediation_fix_step(&task.title)
            || (!explicitly_declares
                && (Self::step_requires_runtime_verification(&task.title)
                    || task.title.to_ascii_lowercase().contains("kit_stack_add")))
        {
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
        let clauses = Self::split_heading_clauses(&task_heads[task_idx]);
        if clauses.is_empty() {
            return false;
        }
        if !explicitly_declares
            && clauses
                .iter()
                .any(|c| Self::clause_starts_with_operational_verb(c))
        {
            return false;
        }

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
        if clauses.len() == 1 {
            owned_files.iter().any(|(rel, stems)| {
                !Self::is_pure_data_helper_for_clause(rel, &clauses[0])
                    && Self::single_clause_segments_align(
                        stems,
                        &clauses[0],
                        &task_lowers[task_idx],
                    )
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
            owned_files.iter().any(|(rel, file_stems)| {
                !Self::is_pure_data_helper_for_clause(rel, clause)
                    && file_stems
                        .iter()
                        .any(|tok| Self::stem_token_matches_task(tok, clause))
            })
        });
        if !any_stem_clause_matched {
            return false;
        }
        clauses.iter().all(|clause| {
            let matched_by_stem = owned_files.iter().any(|(rel, file_stems)| {
                !Self::is_pure_data_helper_for_clause(rel, clause)
                    && file_stems
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
    /// (`" + "` or `", "`), ignoring conjunctions nested inside parenthetical remarks `(...)`
    /// or comparison/example lists (`"Comparison matrix vs Copilot, Cursor, Claude Code"`),
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
            let first_seg = stripped.split(", ").next().unwrap_or("");
            let is_comparison_or_example_list = first_seg.contains(" vs ")
                || first_seg.contains(" versus ")
                || first_seg.contains(" including ")
                || first_seg.contains(" featuring ")
                || first_seg.contains(" such as ");
            if is_comparison_or_example_list {
                vec![stripped.trim()]
            } else {
                stripped.split(", ").map(str::trim).collect()
            }
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

    /// Turn-end / `power_verify` intra-file substep content verification:
    /// When the parent macro task declares explicit target file(s) (e.g. `src/index.css`) that were
    /// modified and verified on disk, any remaining child substep without its own file path is
    /// verified against the parent file's on-disk content.
    ///
    /// NOTE: Intentionally NEVER called from mid-turn `reconcile_preceding_tasks_on_step_transition`
    /// so writing a parent's deliverable file mid-turn leaves the active step `[>]` `DeliverablesReady`
    /// for the agent to call `update_progress` rather than silently closing the entire subplan early.
    fn reconcile_subplan_intra_file_content(&self, modified_files: &[String]) -> usize {
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
                        || Self::step_requires_runtime_verification(&sub.title)
                        || Self::is_remediation_fix_step(&sub.title)
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

        reconciled
    }

    /// Cross-macro progression auto-closure when the active macro task's deliverables (and any
    /// explicit substep deliverables) are verified on disk and `modified_files` modifies a file
    /// owned by a **strictly subsequent** uncompleted macro task (`idx > parent_idx`) in `todo.md`.
    fn reconcile_subplan_on_later_macro_transition(&self, modified_files: &[String]) -> usize {
        if !self.is_tactical_subplan() || modified_files.is_empty() {
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
        if Self::step_requires_runtime_verification(&parent_task.title) {
            return 0;
        }

        let all_subs = self.read_task_plan_tasks();
        let remaining_indices: Vec<usize> = (0..all_subs.len())
            .filter(|&i| all_subs[i].status != TaskItemStatus::Completed)
            .collect();
        if remaining_indices.is_empty() {
            return 0;
        }

        let later_macro_has_verification = m.tasks[(parent_idx + 1)..].iter().any(|t| {
            t.status != TaskItemStatus::Completed
                && Self::step_requires_runtime_verification(&t.title)
        });
        let is_deferrable_smoke_substep = |title: &str| -> bool {
            later_macro_has_verification && Self::is_deferrable_intermediate_smoke_substep(title)
        };

        let effective_sub_cands = self.effective_subplan_task_candidates(&all_subs);
        let subs_have_no_pending_verify_or_missing_files = remaining_indices.iter().all(|&i| {
            let s = &all_subs[i];
            (is_deferrable_smoke_substep(&s.title)
                || (!Self::is_verification_gate_step(&s.title)
                    && !Self::step_requires_runtime_verification(&s.title)))
                && effective_sub_cands[i]
                    .iter()
                    .all(|c| self.is_candidate_completed_on_disk(c))
        });
        if !subs_have_no_pending_verify_or_missing_files {
            return 0;
        }

        let parent_verified = (Self::task_declares_verifiable_deliverables(&parent_task.title)
            && self.are_task_deliverables_verified_on_disk(parent_idx, &m.tasks))
            || (self.is_step_deliverables_satisfied_on_disk(parent_idx, &m.tasks)
                && remaining_indices.iter().all(|&i| {
                    is_deferrable_smoke_substep(&all_subs[i].title)
                        || self.is_step_deliverables_satisfied_on_disk(i, &all_subs)
                }));
        if !parent_verified {
            return 0;
        }

        let all_sub_cands: Vec<String> = effective_sub_cands.into_iter().flatten().collect();
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

        let batch_touches_parent = modified_files.iter().any(|m_path| {
            if !self.is_candidate_completed_on_disk(m_path) {
                return false;
            }
            if macro_cands[parent_idx]
                .iter()
                .chain(all_sub_cands.iter())
                .any(|c| Self::paths_match_by_components(m_path, c))
            {
                return true;
            }
            let stems = Self::split_identifier_stem_tokens(m_path);
            !stems.is_empty()
                && macro_cands[parent_idx].is_empty()
                && Self::is_file_owned_by_task(
                    m_path,
                    &stems,
                    parent_idx,
                    &m.tasks,
                    &macro_heads,
                    &macro_lowers,
                    &macro_cands,
                )
        });
        if batch_touches_parent {
            return 0;
        }

        let preceding_macros_satisfied = |target_macro_idx: usize| -> bool {
            ((parent_idx + 1)..target_macro_idx).all(|k| {
                m.tasks[k].status == TaskItemStatus::Completed
                    || (!Self::step_requires_runtime_verification(&m.tasks[k].title)
                        && self.is_step_deliverables_satisfied_on_disk(k, &m.tasks))
            })
        };

        let moved_to_later_macro = modified_files.iter().any(|m_path| {
            if !self.is_candidate_completed_on_disk(m_path) {
                return false;
            }
            let explicitly_claimed_by_later = macro_cands
                .iter()
                .enumerate()
                .skip(parent_idx + 1)
                .any(|(idx, cands)| {
                    m.tasks[idx].status != TaskItemStatus::Completed
                        && cands
                            .iter()
                            .any(|c| Self::paths_match_by_components(m_path, c))
                        && preceding_macros_satisfied(idx)
                });
            if explicitly_claimed_by_later {
                return true;
            }
            let stems = Self::split_identifier_stem_tokens(m_path);
            !stems.is_empty()
                && ((parent_idx + 1)..m.tasks.len()).any(|idx| {
                    m.tasks[idx].status != TaskItemStatus::Completed
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
                        && preceding_macros_satisfied(idx)
                })
        });

        let mut reconciled = 0usize;
        if moved_to_later_macro {
            for i in remaining_indices {
                if self
                    .update_progress(&all_subs[i].title, "completed")
                    .is_ok()
                {
                    reconciled += 1;
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
        let mut reconciled = self.reconcile_subplan_intra_file_content(modified_files);
        for _ in 0..8 {
            let step_reconciled = self.reconcile_subplan_on_later_macro_transition(modified_files);
            if step_reconciled == 0 {
                break;
            }
            reconciled += step_reconciled;
        }
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
        let task_explicit_candidates = self.effective_subplan_task_candidates(&tasks);

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
                        if Self::is_pure_data_helper_for_clause(m_path, &clauses[0])
                            || !Self::single_clause_segments_align(
                                &stem_tokens,
                                &clauses[0],
                                &task_lowers[task_idx],
                            )
                        {
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

    /// Evaluates package-installation command steps (such as `"bun add @fontsource-variable/jetbrains-mono"`)
    /// directly against `package.json` / `Cargo.toml` on disk so they neither trigger prematurely
    /// before the package is installed nor remain stuck after installation.
    fn check_package_install_step_on_disk(&self, raw_title: &str) -> Option<bool> {
        let trimmed = raw_title.trim();
        let lower = trimmed.to_ascii_lowercase();
        let js_prefixes = [
            "bun add ",
            "npm install ",
            "npm i ",
            "pnpm add ",
            "yarn add ",
        ];
        for prefix in js_prefixes {
            if lower.starts_with(prefix) {
                let rest = &trimmed[prefix.len()..];
                let mut pkgs = Vec::new();
                for tok in rest.split_whitespace() {
                    if matches!(tok, "&&" | ";" | "|" | "||") {
                        break;
                    }
                    if tok.starts_with('-') {
                        continue;
                    }
                    let pkg_name = if let Some(stripped_scope) = tok.strip_prefix('@') {
                        if let Some(ver_idx) = stripped_scope.find('@') {
                            &tok[..1 + ver_idx]
                        } else {
                            tok
                        }
                    } else if let Some(ver_idx) = tok.find('@') {
                        &tok[..ver_idx]
                    } else {
                        tok
                    };
                    let clean = pkg_name.trim_matches(|c: char| {
                        !c.is_ascii_alphanumeric() && !matches!(c, '@' | '/' | '-' | '_' | '.')
                    });
                    if clean.len() >= 2 && clean.chars().any(|c| c.is_ascii_alphabetic()) {
                        pkgs.push(clean.to_string());
                    }
                }
                if pkgs.is_empty() {
                    return None;
                }
                let pkg_json = fs::read_to_string(self.workspace_root.join("package.json")).ok()?;
                return Some(
                    pkgs.iter()
                        .all(|pkg| pkg_json.contains(&format!("\"{pkg}\""))),
                );
            }
        }
        if lower.starts_with("cargo add ") {
            let rest = &trimmed["cargo add ".len()..];
            let mut crates = Vec::new();
            for tok in rest.split_whitespace() {
                if matches!(tok, "&&" | ";" | "|" | "||") {
                    break;
                }
                if tok.starts_with('-') {
                    continue;
                }
                let name = tok.split('@').next().unwrap_or(tok).trim();
                if name.len() >= 2 && name.chars().any(|c| c.is_ascii_alphabetic()) {
                    crates.push(name.to_string());
                }
            }
            if crates.is_empty() {
                return None;
            }
            let cargo_toml = fs::read_to_string(self.workspace_root.join("Cargo.toml")).ok()?;
            return Some(crates.iter().all(|c| cargo_toml.contains(c.as_str())));
        }
        None
    }

    /// Checks whether `tasks[task_idx]`'s file/DOM/foundation deliverables exist on disk
    /// (independent of whether the step additionally requires runtime verification).
    pub(super) fn is_step_deliverables_satisfied_on_disk(
        &self,
        task_idx: usize,
        tasks: &[TaskItem],
    ) -> bool {
        let Some(task) = tasks.get(task_idx) else {
            return false;
        };
        if let Some(installed) = self.check_package_install_step_on_disk(&task.title) {
            return installed;
        }
        if self.are_task_deliverables_verified_on_disk(task_idx, tasks) {
            return true;
        }
        let candidates = Self::extract_candidate_files(&task.title);
        let lower_title = task.title.to_ascii_lowercase();
        if candidates.is_empty()
            && !Self::step_requires_runtime_verification(&task.title)
            && !Self::is_remediation_fix_step(&task.title)
            && (lower_title.contains("kit_stack_add") || lower_title.contains("onpkg stack add"))
            && crate::agent::orchestrator::WorkflowRouter::project_manifest(&self.workspace_root)
                .is_some()
        {
            return true;
        }
        if candidates.is_empty()
            && task_idx == 0
            && !Self::task_declares_verifiable_deliverables(&task.title)
            && !Self::step_requires_runtime_verification(&task.title)
            && !Self::is_remediation_fix_step(&task.title)
        {
            let is_root_macro_step_zero = !self.is_tactical_subplan()
                || self
                    .read_roadmap_milestones()
                    .into_iter()
                    .find(|m| m.is_active)
                    .map(|m| {
                        m.tasks
                            .first()
                            .is_some_and(|first_m| first_m.title == task.title)
                            || m.tasks
                                .iter()
                                .position(|t| t.status != TaskItemStatus::Completed)
                                == Some(0)
                    })
                    .unwrap_or(false);
            let primary_head = Self::extract_primary_deliverable_head(&task.title);
            let head_clauses = Self::split_heading_clauses(&primary_head);
            let is_foundation_or_setup_step = head_clauses
                .iter()
                .any(|c| Self::clause_starts_with_operational_verb(c))
                || lower_title.starts_with("foundation")
                || lower_title.contains("kit_stack_add")
                || lower_title.contains("boilerplate");
            if is_root_macro_step_zero
                && is_foundation_or_setup_step
                && crate::agent::orchestrator::WorkflowRouter::project_manifest(
                    &self.workspace_root,
                )
                .is_some()
            {
                return true;
            }
        }
        // Intra-file substep content verification in `Mode: subplan` when the active parent macro
        // task declares explicit file(s) (e.g. `src/index.css`) and this substep has no file of its own.
        if self.is_tactical_subplan()
            && candidates.is_empty()
            && !Self::is_verification_gate_step(&task.title)
            && !Self::step_requires_runtime_verification(&task.title)
            && !Self::is_remediation_fix_step(&task.title)
        {
            let effective_cands = self.effective_subplan_task_candidates(tasks);
            if effective_cands.get(task_idx).is_some_and(Vec::is_empty) {
                let sub_tasks = self.read_task_plan_tasks();
                let is_active_subplan_slice = sub_tasks.len() == tasks.len()
                    && sub_tasks
                        .iter()
                        .zip(tasks.iter())
                        .all(|(a, b)| a.title == b.title);
                if is_active_subplan_slice {
                    if let Some(parent_task) = self
                        .read_roadmap_milestones()
                        .into_iter()
                        .find(|m| m.is_active)
                        .and_then(|m| {
                            m.tasks
                                .iter()
                                .find(|t| t.status == TaskItemStatus::InProgress)
                                .cloned()
                                .or_else(|| {
                                    m.tasks
                                        .into_iter()
                                        .find(|t| t.status == TaskItemStatus::Pending)
                                })
                        })
                    {
                        let parent_cands = Self::extract_candidate_files(&parent_task.title);
                        if !parent_cands.is_empty()
                            && parent_cands
                                .iter()
                                .all(|c| self.is_candidate_completed_on_disk(c))
                        {
                            let tokens = Self::extract_substep_content_tokens(&task.title);
                            if !tokens.is_empty() {
                                let mut parent_disk_lower = String::new();
                                for cand in &parent_cands {
                                    if let Some(resolved) = self.resolve_candidate_on_disk(cand) {
                                        if let Ok(text) = fs::read_to_string(&resolved) {
                                            parent_disk_lower.push_str(&text.to_ascii_lowercase());
                                            parent_disk_lower.push('\n');
                                        }
                                    }
                                }
                                let matched = tokens
                                    .iter()
                                    .filter(|tok| parent_disk_lower.contains(tok.as_str()))
                                    .count();
                                if matched * 2 >= tokens.len() {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
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
        let mut reconciled = 0usize;
        for _ in 0..8 {
            let step_reconciled = self.reconcile_subplan_on_later_macro_transition(modified_files);
            if step_reconciled == 0 {
                break;
            }
            reconciled += step_reconciled;
        }
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
        let task_explicit_candidates = self.effective_subplan_task_candidates(&tasks);

        let mut html_section_ids = std::collections::HashSet::new();
        for m_path in modified_files {
            if !self.is_candidate_completed_on_disk(m_path) {
                continue;
            }
            if m_path.ends_with(".html") || m_path.ends_with(".htm") {
                let full_path = self.workspace_root.join(m_path.trim_start_matches("./"));
                if let Ok(content) = fs::read_to_string(&full_path) {
                    html_section_ids.extend(Self::extract_html_section_ids(&content));
                }
            }
        }

        // If any modified file in this batch belongs to the currently active step `first_uncompleted`,
        // the agent is still actively building `first_uncompleted` in this turn — do not prematurely
        // close it from a sibling stub file written in the same batch.
        let active_step_touched_in_batch = modified_files.iter().any(|m_path| {
            if !self.is_candidate_completed_on_disk(m_path) {
                return false;
            }
            let is_html = m_path.ends_with(".html") || m_path.ends_with(".htm");
            if is_html {
                return false;
            }
            if task_explicit_candidates[first_uncompleted]
                .iter()
                .any(|c| Self::paths_match_by_components(m_path, c))
            {
                return true;
            }
            if !task_explicit_candidates[first_uncompleted].is_empty()
                || Self::is_verification_gate_step(&tasks[first_uncompleted].title)
            {
                return false;
            }
            let stems = Self::split_identifier_stem_tokens(m_path);
            !stems.is_empty()
                && Self::is_file_owned_by_task(
                    m_path,
                    &stems,
                    first_uncompleted,
                    &tasks,
                    &task_heads,
                    &task_lowers,
                    &task_explicit_candidates,
                )
        });
        if active_step_touched_in_batch {
            return reconciled;
        }

        let mut candidate_targets: std::collections::BTreeSet<usize> =
            std::collections::BTreeSet::new();

        for m_path in modified_files {
            if !self.is_candidate_completed_on_disk(m_path) {
                continue;
            }
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
                    candidate_targets.insert(target_idx);
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
                        candidate_targets.insert(target_idx);
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
                    && (Self::is_pure_data_helper_for_clause(m_path, &clauses[0])
                        || !Self::single_clause_segments_align(
                            &stem_tokens,
                            &clauses[0],
                            &task_lowers[target_idx],
                        ))
                {
                    continue;
                }
                candidate_targets.insert(target_idx);
            }
        }

        // Only treat `target_idx` as a genuine sequential step transition if EVERY intermediate step
        // in `first_uncompleted..target_idx` is already Completed or has its deliverables satisfied on disk
        // (preventing an out-of-order stub file for Step 5 from closing Step 2 while Steps 3 & 4 are unstarted).
        let valid_target_idx = candidate_targets.iter().rev().copied().find(|&target_idx| {
            (first_uncompleted..target_idx).all(|prev_idx| {
                tasks[prev_idx].status == TaskItemStatus::Completed
                    || (!Self::step_requires_runtime_verification(&tasks[prev_idx].title)
                        && self.is_step_deliverables_satisfied_on_disk(prev_idx, &tasks))
            })
        });

        if let Some(target_idx) = valid_target_idx {
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
