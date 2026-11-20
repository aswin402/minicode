use crate::constants::GIT_COMMIT_MSG_MAX_LEN;
use crate::error::{GitError, Result};
use crate::git::service::GitService;

/// Service for committing, branching, and rolling back git changes.
pub struct GitCommitService<'a> {
    git: &'a GitService,
    /// When true, `commit` stages ALL workspace changes (`git add -A`)
    /// instead of only the explicitly listed paths (dirty_commit config).
    stage_all: bool,
}

impl<'a> GitCommitService<'a> {
    /// Creates a new GitCommitService backed by the given GitService.
    pub fn new(git: &'a GitService) -> Self {
        Self {
            git,
            stage_all: false,
        }
    }

    /// Builder: stage every workspace change, not just the listed files.
    pub fn with_stage_all(mut self, yes: bool) -> Self {
        self.stage_all = yes;
        self
    }

    /// Stages modified files and returns the exact canonical list of paths currently staged in the Git index.
    /// Excludes gitignored files and transient created-then-deleted files automatically via Git's index.
    pub async fn stage_and_list_staged_files(
        &self,
        paths: Option<&[String]>,
    ) -> Result<Vec<String>> {
        if self.stage_all {
            self.git.run_git(&["add", "-A"]).await?;
        } else {
            self.git.stage_files(paths).await?;
        }

        match self.git.run_git(&["diff", "--cached", "--name-only"]).await {
            Ok(out) => {
                let staged: Vec<String> = out
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
                Ok(staged)
            }
            Err(_) => {
                // Fallback for unborn branches on older Git versions
                let status = self.git.get_status().await?;
                Ok(status.staged)
            }
        }
    }

    /// Commits whatever changes are currently staged in the Git index (`git commit -m`),
    /// returning `Ok(None)` when the index has nothing staged to commit.
    pub async fn commit_staged(&self, message: &str) -> Result<Option<String>> {
        let trimmed_msg = message.trim();
        if trimmed_msg.is_empty() {
            return Err(GitError::CommandFailed {
                cmd: "git commit".into(),
                code: None,
                stderr: "Commit message cannot be empty".into(),
            }
            .into());
        }

        match self.git.run_git(&["commit", "-m", trimmed_msg]).await {
            Ok(_) => {}
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("nothing to commit")
                    || err_str.contains("working tree clean")
                    || err_str.contains("no changes added to commit")
                {
                    return Ok(None);
                }
                return Err(e);
            }
        }

        let hash = self.git.run_git(&["rev-parse", "HEAD"]).await?;
        Ok(Some(hash.trim().to_string()))
    }

    /// Stages modified files and creates a git commit only when staged diffs exist.
    /// Returns `Ok(None)` when the working tree has nothing new to commit after staging.
    pub async fn commit_if_dirty(
        &self,
        message: &str,
        paths: Option<&[String]>,
    ) -> Result<Option<String>> {
        if message.trim().is_empty() {
            return Err(GitError::CommandFailed {
                cmd: "git commit".into(),
                code: None,
                stderr: "Commit message cannot be empty".into(),
            }
            .into());
        }

        let staged = self.stage_and_list_staged_files(paths).await?;
        if staged.is_empty() {
            return Ok(None);
        }
        self.commit_staged(message).await
    }

    /// Stages modified files and creates a git commit, returning the created commit hash.
    pub async fn commit(&self, message: &str, paths: Option<&[String]>) -> Result<String> {
        match self.commit_if_dirty(message, paths).await? {
            Some(hash) => Ok(hash),
            None => {
                let hash = self
                    .git
                    .run_git(&["rev-parse", "HEAD"])
                    .await
                    .unwrap_or_else(|_| "HEAD".to_string());
                Ok(format!(
                    "{} (clean working tree, nothing new to commit)",
                    hash.trim()
                ))
            }
        }
    }

    /// Rolls back the latest commit while preserving all modified files in the working tree (`git reset --soft HEAD~1`).
    pub async fn undo_last_commit(&self) -> Result<()> {
        let parent_check = self.git.run_git(&["rev-parse", "HEAD~1"]).await;
        if parent_check.is_err() {
            return Err(GitError::NoCommits.into());
        }

        self.git.run_git(&["reset", "--soft", "HEAD~1"]).await?;
        Ok(())
    }

    /// Creates and switches to a new git branch.
    pub async fn create_branch(&self, branch_name: &str) -> Result<()> {
        self.git.run_git(&["checkout", "-b", branch_name]).await?;
        Ok(())
    }

    /// Switches to an existing git branch.
    pub async fn checkout_branch(&self, branch_name: &str) -> Result<()> {
        self.git.run_git(&["checkout", branch_name]).await?;
        Ok(())
    }

    /// Lists all local branch names.
    pub async fn list_branches(&self) -> Result<Vec<String>> {
        let output = self
            .git
            .run_git(&["branch", "--list", "--format=%(refname:short)"])
            .await?;
        let branches = output
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        Ok(branches)
    }

    /// Extracts the raw user prompt body when wrapped in `<user_request>...</user_request>`
    /// so injected workspace telemetry headers in `Role::User` messages are never scraped.
    pub fn extract_user_request_body(raw: &str) -> &str {
        const OPEN_TAG: &str = "<user_request>";
        const CLOSE_TAG: &str = "</user_request>";
        if let Some(open_idx) = raw.rfind(OPEN_TAG) {
            let after_open = &raw[open_idx + OPEN_TAG.len()..];
            if let Some(close_idx) = after_open.find(CLOSE_TAG) {
                return after_open[..close_idx].trim();
            }
            return after_open.trim();
        }
        raw.trim()
    }

    /// Extracts a clean, single-line primary headline from a user prompt or plan title.
    /// Returns `None` if the input is empty or is a single-word continuation/acknowledgement
    /// cue (e.g. `"continue"`, `"proceed"`, `"ok"`, `"yes"`).
    pub fn extract_meaningful_headline(raw: &str) -> Option<String> {
        let raw = Self::extract_user_request_body(raw);
        let trimmed_raw = raw.trim();
        let upper_raw = trimmed_raw.to_ascii_uppercase();
        if upper_raw.starts_with("[SYSTEM")
            || upper_raw.starts_with("SYSTEM -")
            || upper_raw.starts_with("<COMPLETED_BACKGROUND")
            || upper_raw.starts_with("<SYSTEM")
            || upper_raw.starts_with("[ORCHESTRATOR")
            || upper_raw.contains("MINIPUB BOOMERANG COMPLETION REPORT")
            || upper_raw.starts_with("# DYNAMIC WORKSPACE CONTEXT")
            || upper_raw.starts_with("# TOOL REGISTRY TELEMETRY")
            || upper_raw.starts_with("REGISTRY SUMMARY:")
        {
            return None;
        }

        let first_line = raw.lines().map(str::trim).find(|l| {
            let up = l.to_ascii_uppercase();
            !l.is_empty()
                && !l.starts_with("```")
                && !l.starts_with('<')
                && !up.starts_with("[SYSTEM")
                && !up.starts_with("REGISTRY SUMMARY:")
                && !up.starts_with("# DYNAMIC WORKSPACE CONTEXT")
                && !up.starts_with("# TOOL REGISTRY TELEMETRY")
        })?;

        // Strip markdown inline decorators (*, `, #, >) and normalize whitespace
        let stripped: String = first_line
            .chars()
            .filter(|&c| !matches!(c, '*' | '`' | '#' | '>'))
            .collect();
        let first_sentence = stripped
            .split(". ")
            .next()
            .unwrap_or(&stripped)
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim();

        let word_count = first_sentence
            .split_whitespace()
            .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
            .count();
        if word_count < 2 {
            return None;
        }
        Some(first_sentence.to_string())
    }

    /// Generates a Conventional Commit message from modified file paths and optional turn summary.
    pub fn generate_conventional_message(
        files_modified: &[String],
        task_summary: Option<&str>,
    ) -> String {
        let headline = task_summary.and_then(Self::extract_meaningful_headline);
        if files_modified.is_empty() {
            return headline.unwrap_or_else(|| "chore: update workspace".to_string());
        }

        // Determine scope and prefix based on file patterns
        let has_tests = files_modified
            .iter()
            .any(|f| f.contains("test") || f.ends_with("_test.rs") || f.contains(".spec."));
        let has_docs = files_modified
            .iter()
            .any(|f| f.ends_with(".md") || f.starts_with("docs/"));
        let has_src = files_modified.iter().any(|f| {
            f.starts_with("src/")
                || f.ends_with(".rs")
                || f.ends_with(".ts")
                || f.ends_with(".tsx")
                || f.ends_with(".js")
                || f.ends_with(".jsx")
                || f.ends_with(".py")
                || f.ends_with(".go")
                || f.ends_with(".html")
                || f.ends_with(".css")
        });
        let has_config = files_modified.iter().any(|f| {
            f.ends_with(".toml")
                || f.ends_with(".json")
                || f.ends_with(".yml")
                || f.ends_with(".yaml")
        });

        let prefix = if let Some(ref head) = headline {
            let lead_tokens: Vec<String> = head
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .take(8)
                .map(|w| w.to_ascii_lowercase())
                .collect();

            let has_lead =
                |candidates: &[&str]| lead_tokens.iter().any(|t| candidates.contains(&t.as_str()));

            if has_lead(&[
                "create",
                "build",
                "implement",
                "add",
                "scaffold",
                "design",
                "develop",
                "introduce",
                "ship",
            ]) {
                "feat"
            } else if has_lead(&[
                "fix", "bug", "bugfix", "hotfix", "repair", "resolve", "patch", "crash", "error",
                "broken",
            ]) {
                "fix"
            } else if has_lead(&["refactor", "restructure", "simplify", "clean", "cleanup"]) {
                "refactor"
            } else if has_lead(&["test", "tests", "testing", "spec", "coverage"]) {
                "test"
            } else if has_lead(&["doc", "docs", "document", "readme", "changelog"]) {
                "docs"
            } else if has_src {
                "feat"
            } else if has_tests {
                "test"
            } else if has_docs {
                "docs"
            } else {
                "chore"
            }
        } else if has_tests && !has_src {
            "test"
        } else if has_docs && !has_src {
            "docs"
        } else if has_config && !has_src {
            "chore"
        } else {
            "feat"
        };

        // Determine concise summary (clean word/clause boundary, never slicing mid-word)
        let summary_text = if let Some(ref head) = headline {
            crate::utils::strings::truncate_words(head, GIT_COMMIT_MSG_MAX_LEN - 15)
        } else if files_modified.len() == 1 {
            format!("update {}", files_modified[0])
        } else {
            format!("update {} files", files_modified.len())
        };

        format!("{}: {}", prefix, summary_text)
    }

    /// Infers a concise Conventional Commit `(scope)` from modified workspace paths without hardcoding
    /// project-specific names. Returns `None` when changes span many unrelated subsystems.
    pub fn infer_conventional_scope(files_modified: &[String]) -> Option<String> {
        let clean_files: Vec<&str> = files_modified
            .iter()
            .map(|f| f.trim_start_matches("./"))
            .filter(|f| !f.is_empty() && !f.starts_with(".minicode/"))
            .collect();
        if clean_files.is_empty() {
            return None;
        }

        if clean_files.len() == 1 {
            let single = clean_files[0];
            if single.eq_ignore_ascii_case("readme.md") {
                return Some("readme".to_string());
            }
            if let Some(rest) = single.strip_prefix("src/") {
                if let Some((top_mod, _)) = rest.split_once('/') {
                    return Some(top_mod.to_ascii_lowercase());
                }
                if let Some((stem, _)) = rest.rsplit_once('.') {
                    if stem != "main" && stem != "lib" && stem != "mod" && stem != "index" {
                        return Some(stem.to_ascii_lowercase());
                    }
                }
            }
            if single.starts_with("tests/") {
                return Some("tests".to_string());
            }
            if single.starts_with("docs/") || single.starts_with("minikit_docs/") {
                return Some("docs".to_string());
            }
        }

        let code_files: Vec<&str> = clean_files
            .iter()
            .copied()
            .filter(|f| {
                !f.ends_with(".md")
                    && !f.starts_with("docs/")
                    && !f.starts_with("minikit_docs/")
                    && !f.ends_with(".lock")
            })
            .collect();
        let target_slice = if code_files.is_empty() {
            &clean_files
        } else {
            &code_files
        };

        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut sub_counts: std::collections::BTreeMap<(String, String), usize> =
            std::collections::BTreeMap::new();
        let mut categorized_total = 0usize;
        let mut ui_like_count = 0usize;
        for f in target_slice {
            if f.ends_with(".tsx")
                || f.ends_with(".jsx")
                || f.ends_with(".css")
                || f.ends_with(".html")
                || f.ends_with(".vue")
                || f.ends_with(".svelte")
                || f.starts_with("src/ui/")
                || f.starts_with("src/components/")
                || f.starts_with("src/pages/")
                || f.starts_with("src/layouts/")
            {
                ui_like_count += 1;
            }
            let bucket = if let Some(rest) = f.strip_prefix("src/") {
                let mut segs = rest.split('/');
                let first = segs
                    .next()
                    .unwrap_or("core")
                    .trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                    .to_ascii_lowercase();
                // Record second-level feature directory (`src/<bucket>/<subscope>/<file>`)
                if let (Some(second), Some(_third)) = (segs.next(), segs.next()) {
                    let clean_sub = second
                        .trim_end_matches(|c: char| {
                            !c.is_ascii_alphanumeric() && c != '_' && c != '-'
                        })
                        .to_ascii_lowercase();
                    if !clean_sub.is_empty()
                        && !matches!(
                            clean_sub.as_str(),
                            "ui" | "common"
                                | "shared"
                                | "utils"
                                | "sections"
                                | "layout"
                                | "layouts"
                                | "partials"
                                | "widgets"
                                | "blocks"
                                | "primitives"
                                | "elements"
                        )
                    {
                        *sub_counts.entry((first.clone(), clean_sub)).or_insert(0) += 1;
                    }
                }
                first
            } else if f.starts_with("tests/") {
                "tests".to_string()
            } else if f.starts_with("docs/") || f.starts_with("minikit_docs/") || f.ends_with(".md")
            {
                "docs".to_string()
            } else {
                continue;
            };
            if !bucket.is_empty()
                && !matches!(
                    bucket.as_str(),
                    "main" | "lib" | "mod" | "index" | "app" | "types"
                )
            {
                *counts.entry(bucket).or_insert(0) += 1;
                categorized_total += 1;
            }
        }

        let total = target_slice.len();
        let denom = if categorized_total >= 3 && categorized_total * 2 >= total {
            categorized_total
        } else {
            total
        };
        if denom > 0 {
            if let Some((best_scope, best_count)) = counts.iter().max_by_key(|(_, c)| *c) {
                if *best_count * 100 >= denom * 60 {
                    let is_generic_container = matches!(
                        best_scope.as_str(),
                        "components"
                            | "modules"
                            | "features"
                            | "views"
                            | "pages"
                            | "routes"
                            | "containers"
                    );
                    if is_generic_container {
                        if let Some(((_, best_sub), sub_count)) = sub_counts
                            .iter()
                            .filter(|((parent, _), _)| parent == best_scope)
                            .max_by_key(|(_, c)| *c)
                        {
                            if *sub_count * 100 >= *best_count * 75 {
                                return Some(best_sub.clone());
                            }
                        }
                        if best_scope == "components" && ui_like_count * 100 >= total * 60 {
                            return Some("ui".to_string());
                        }
                    }
                    return Some(best_scope.clone());
                }
            }
        }
        if total >= 2 && ui_like_count * 100 >= total * 65 {
            return Some("ui".to_string());
        }
        None
    }

    /// Synthesizes a clean, idiomatic Conventional Commit message (`type(scope): lowercase imperative subject`
    /// plus a compact grouped file summary body for multi-file commits) for autonomous turn commits.
    pub fn synthesize_turn_commit_message(
        files_modified: &[String],
        plan_title: Option<&str>,
        user_prompt: Option<&str>,
    ) -> String {
        let prompt_headline = user_prompt.and_then(Self::extract_meaningful_headline);
        let plan_headline = plan_title.and_then(Self::extract_meaningful_headline);

        // Determine Conventional Commit type using whichever headline carries explicit intent verbs
        let base_for_type = prompt_headline.as_deref().or(plan_headline.as_deref());
        let legacy_typed = Self::generate_conventional_message(files_modified, base_for_type);
        let commit_type = legacy_typed
            .split_once(':')
            .map(|(t, _)| t.trim())
            .unwrap_or("feat");

        let scope = Self::infer_conventional_scope(files_modified);
        let header_prefix = match &scope {
            Some(s) if !s.is_empty() => format!("{commit_type}({s}): "),
            _ => format!("{commit_type}: "),
        };
        let max_subject_chars = GIT_COMMIT_MSG_MAX_LEN
            .saturating_sub(header_prefix.chars().count())
            .max(24);

        let raw_candidate = match (&plan_headline, &prompt_headline) {
            (Some(plan_h), Some(prompt_h)) => {
                // Prefer prompt headline when concise, or trim prompt at natural clause boundary;
                // fall back to plan title when prompt is overly verbose.
                let clause_trimmed = Self::trim_at_natural_clause(prompt_h, max_subject_chars);
                if clause_trimmed.chars().count() <= max_subject_chars {
                    Some(clause_trimmed)
                } else {
                    Some(Self::trim_at_natural_clause(plan_h, max_subject_chars))
                }
            }
            (Some(plan_h), None) => Some(Self::trim_at_natural_clause(plan_h, max_subject_chars)),
            (None, Some(prompt_h)) => {
                Some(Self::trim_at_natural_clause(prompt_h, max_subject_chars))
            }
            (None, None) => None,
        };

        let subject = if let Some(candidate) = raw_candidate {
            let imperative = Self::ensure_imperative_lowercase_subject(&candidate, commit_type);
            let bounded = if imperative.chars().count() > max_subject_chars {
                crate::utils::strings::truncate_words(&imperative, max_subject_chars)
            } else {
                imperative
            };
            Self::strip_unbalanced_trailing_delimiters(&bounded)
        } else if files_modified.len() == 1 {
            format!("update {}", files_modified[0])
        } else if files_modified.is_empty() {
            "update workspace".to_string()
        } else {
            format!("update {} workspace files", files_modified.len())
        };

        let header = format!("{header_prefix}{subject}");
        let body = Self::format_grouped_files_body(files_modified);
        if body.is_empty() {
            header
        } else {
            format!("{header}\n\n{body}")
        }
    }

    fn has_unbalanced_delimiters(s: &str) -> bool {
        let mut parens = 0i32;
        let mut brackets = 0i32;
        let mut backticks = 0usize;
        for ch in s.chars() {
            match ch {
                '(' => parens += 1,
                ')' => parens -= 1,
                '[' => brackets += 1,
                ']' => brackets -= 1,
                '`' => backticks += 1,
                _ => {}
            }
        }
        parens != 0 || brackets != 0 || !backticks.is_multiple_of(2)
    }

    fn strip_unbalanced_trailing_delimiters(s: &str) -> String {
        let mut out = s
            .trim_end_matches(['.', ',', ';', ':', '…', '-', '—'])
            .trim()
            .to_string();
        for (open, close) in [('(', ')'), ('[', ']')] {
            loop {
                let opens = out.chars().filter(|&c| c == open).count();
                let closes = out.chars().filter(|&c| c == close).count();
                if opens > closes {
                    if let Some(pos) = out.rfind(open) {
                        let prefix = out[..pos]
                            .trim_end_matches(['.', ',', ';', ':', '…', '-', '—'])
                            .trim();
                        if !prefix.is_empty() {
                            out = prefix.to_string();
                            continue;
                        }
                    }
                }
                break;
            }
        }
        if !out.chars().filter(|&c| c == '`').count().is_multiple_of(2) {
            if let Some(pos) = out.rfind('`') {
                let prefix = out[..pos]
                    .trim_end_matches(['.', ',', ';', ':', '…', '-', '—'])
                    .trim();
                if !prefix.is_empty() {
                    out = prefix.to_string();
                }
            }
        }
        out
    }

    fn trim_at_natural_clause(text: &str, max_chars: usize) -> String {
        let mut cleaned = text.trim();
        for pfx in [
            "feat:",
            "fix:",
            "refactor:",
            "docs:",
            "test:",
            "chore:",
            "perf:",
            "build:",
        ] {
            if cleaned.len() >= pfx.len() && cleaned[..pfx.len()].eq_ignore_ascii_case(pfx) {
                cleaned = cleaned[pfx.len()..].trim();
                break;
            }
        }
        let cleaned = cleaned.trim_end_matches(['.', ',', ';', ':']).trim();
        if cleaned.chars().count() <= max_chars {
            return cleaned.to_string();
        }

        let mut best_clause: Option<&str> = None;
        for sep in [", ", " — ", " - ", " (", "; ", " built ", " featuring "] {
            for (idx, _) in cleaned.match_indices(sep) {
                let left_trim = cleaned[..idx].trim_end_matches(['.', ',', ';', ':']).trim();
                if Self::has_unbalanced_delimiters(left_trim) {
                    continue;
                }
                let char_len = left_trim.chars().count();
                let words = left_trim.split_whitespace().count();
                if words >= 4
                    && char_len <= max_chars
                    && best_clause.is_none_or(|prev| char_len > prev.chars().count())
                {
                    best_clause = Some(left_trim);
                }
            }
        }
        if let Some(best) = best_clause {
            return best.to_string();
        }
        cleaned.to_string()
    }

    fn ensure_imperative_lowercase_subject(candidate: &str, commit_type: &str) -> String {
        let trimmed = candidate
            .trim()
            .trim_end_matches(['.', ',', ';', ':'])
            .trim();
        if trimmed.is_empty() {
            return "update workspace".to_string();
        }
        let first_word = trimmed.split_whitespace().next().unwrap_or("");
        let first_clean: String = first_word
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_ascii_lowercase();

        const IMPERATIVE_VERBS: &[&str] = &[
            "add",
            "allow",
            "align",
            "apply",
            "audit",
            "avoid",
            "bootstrap",
            "build",
            "bump",
            "clean",
            "clear",
            "close",
            "complete",
            "configure",
            "consolidate",
            "convert",
            "create",
            "deduplicate",
            "define",
            "delete",
            "design",
            "detect",
            "disable",
            "document",
            "drop",
            "eliminate",
            "emit",
            "enable",
            "enforce",
            "enhance",
            "ensure",
            "expand",
            "expose",
            "extend",
            "extract",
            "filter",
            "fix",
            "format",
            "gate",
            "generate",
            "guard",
            "handle",
            "harden",
            "implement",
            "improve",
            "include",
            "init",
            "initialize",
            "inject",
            "integrate",
            "introduce",
            "isolate",
            "keep",
            "limit",
            "link",
            "load",
            "make",
            "map",
            "merge",
            "migrate",
            "modernize",
            "move",
            "normalize",
            "optimize",
            "overhaul",
            "parse",
            "patch",
            "persist",
            "polish",
            "populate",
            "prefer",
            "preserve",
            "prevent",
            "promote",
            "protect",
            "prune",
            "reconcile",
            "record",
            "redesign",
            "reduce",
            "refactor",
            "refine",
            "register",
            "remove",
            "rename",
            "render",
            "repair",
            "replace",
            "require",
            "reset",
            "resolve",
            "restore",
            "restructure",
            "rewrite",
            "route",
            "sanitize",
            "save",
            "scaffold",
            "secure",
            "separate",
            "ship",
            "simplify",
            "skip",
            "sort",
            "split",
            "stabilize",
            "standardize",
            "streamline",
            "strip",
            "style",
            "support",
            "suppress",
            "switch",
            "sync",
            "synthesize",
            "test",
            "tighten",
            "track",
            "trim",
            "truncate",
            "tune",
            "unify",
            "update",
            "upgrade",
            "use",
            "validate",
            "verify",
            "wire",
            "wrap",
        ];

        if IMPERATIVE_VERBS.contains(&first_clean.as_str()) {
            let mut chars = trimmed.chars();
            if let Some(first_ch) = chars.next() {
                return format!("{}{}", first_ch.to_ascii_lowercase(), chars.as_str());
            }
        }

        // Noun-phrase title (e.g. "minicode landing page"): prepend a crisp verb matching commit_type
        let lowered_lead = {
            let is_acronym = first_word
                .chars()
                .filter(|c| c.is_ascii_uppercase())
                .count()
                >= 2
                || first_word.contains('/')
                || first_word.contains('.');
            if is_acronym {
                trimmed.to_string()
            } else {
                let mut chars = trimmed.chars();
                match chars.next() {
                    Some(c) => format!("{}{}", c.to_ascii_lowercase(), chars.as_str()),
                    None => String::new(),
                }
            }
        };
        let default_verb = match commit_type {
            "fix" => "resolve",
            "docs" => "document",
            "test" => "verify",
            "refactor" => "refactor",
            "perf" => "optimize",
            "chore" | "build" | "ci" => "update",
            _ => "implement",
        };
        format!("{default_verb} {lowered_lead}")
    }

    fn format_grouped_files_body(files_modified: &[String]) -> String {
        let clean: Vec<&str> = files_modified
            .iter()
            .map(|f| f.trim_start_matches("./"))
            .filter(|f| !f.is_empty() && !f.starts_with(".minicode/"))
            .collect();
        if clean.len() < 2 {
            return String::new();
        }

        let mut groups: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for path in &clean {
            let (group, leaf) = if let Some((dir, file)) = path.rsplit_once('/') {
                let top = dir.split('/').take(2).collect::<Vec<_>>().join("/");
                (top, file.to_string())
            } else {
                ("root".to_string(), (*path).to_string())
            };
            let entry = groups.entry(group).or_default();
            if !entry.contains(&leaf) {
                entry.push(leaf);
            }
        }

        let mut lines = Vec::new();
        for (group, names) in groups.iter().take(5) {
            let preview = if names.len() <= 3 {
                names.join(", ")
            } else {
                format!(
                    "{} (+{} more)",
                    names[..3].join(", "),
                    names.len().saturating_sub(3)
                )
            };
            lines.push(format!("- {group}: {preview}"));
        }
        if groups.len() > 5 {
            lines.push(format!(
                "- ... +{} additional directories ({} total files)",
                groups.len().saturating_sub(5),
                clean.len()
            ));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_conventional_message_with_task() {
        let files = vec!["src/agent/loop.rs".to_string()];
        let msg =
            GitCommitService::generate_conventional_message(&files, Some("Fix streaming tool bug"));
        assert_eq!(msg, "fix: Fix streaming tool bug");
    }

    #[test]
    fn test_generate_conventional_message_docs_only() {
        let files = vec!["README.md".to_string()];
        let msg = GitCommitService::generate_conventional_message(&files, None);
        assert_eq!(msg, "docs: update README.md");
    }

    #[test]
    fn test_generate_conventional_message_ignores_body_bug_mentions_and_single_word_continue() {
        let files = vec![
            "src/components/Hero.tsx".to_string(),
            "src/index.css".to_string(),
        ];
        let prompt = "Create a premium, modern landing page for **minicode**, an autonomous AI coding agent.\n\nExplain that minicode does not simply claim a bug is fixed without verification.";
        let msg = GitCommitService::generate_conventional_message(&files, Some(prompt));
        assert!(msg.starts_with("feat: Create a premium, modern landing page for minicode"));
        assert!(!msg.contains("**"));

        let cont_msg = GitCommitService::generate_conventional_message(&files, Some("continue"));
        assert_eq!(cont_msg, "feat: update 2 files");
    }

    #[test]
    fn test_generate_conventional_message_minitest50_never_cuts_mid_word() {
        let files = vec!["src/pages/HomePage.tsx".to_string()];
        let prompt = "Create a premium, modern landing page for **minicode**, an autonomous AI coding agent built in Rust.";
        let msg = GitCommitService::generate_conventional_message(&files, Some(prompt));
        assert_eq!(
            msg,
            "feat: Create a premium, modern landing page for minicode…"
        );
        assert!(!msg.ends_with("an a…"));
    }

    #[test]
    fn test_minitest65_extract_user_request_body_strips_telemetry_and_synthesizes_clean_commit() {
        let wrapped_greeting = "# Dynamic Workspace Context\n## Tool Registry Telemetry\nRegistry Summary: 10 Active Tool Schemas in Context | 40+ Deferred Categories\n\n<user_request>\nhii\n</user_request>";
        assert_eq!(
            GitCommitService::extract_user_request_body(wrapped_greeting),
            "hii"
        );
        assert_eq!(
            GitCommitService::extract_meaningful_headline(wrapped_greeting),
            None
        );

        let wrapped_feature = "# Dynamic Workspace Context\nRegistry Summary: 10 Active Tool Schemas in Context\n<user_request>\nCreate a premium, modern landing page for **minicode**, an autonomous AI coding agent built in Rust.\n</user_request>";
        let files = vec![
            "src/components/sections/HeroSection.tsx".to_string(),
            "src/components/sections/SwarmSection.tsx".to_string(),
            "src/pages/HomePage.tsx".to_string(),
            "src/index.css".to_string(),
        ];
        let full_msg = GitCommitService::synthesize_turn_commit_message(
            &files,
            Some("minicode landing page"),
            Some(wrapped_feature),
        );
        let subject = full_msg.lines().next().unwrap();
        assert_eq!(
            subject,
            "feat(ui): create a premium, modern landing page for minicode"
        );
        assert!(!subject.contains('…'));
        assert!(!full_msg.contains("Registry Summary"));
        assert!(full_msg.contains("- src/components: HeroSection.tsx, SwarmSection.tsx"));

        // Noun-phrase plan title alone receives an imperative verb and inferred scope
        let noun_only = GitCommitService::synthesize_turn_commit_message(
            &["src/agent/loop.rs".to_string()],
            Some("minicode landing page"),
            Some("continue"),
        );
        assert_eq!(noun_only, "feat(agent): implement minicode landing page");
    }

    #[tokio::test]
    async fn test_minitest65_commit_if_dirty_returns_none_on_clean_tree() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitService::new(temp.path().to_path_buf());
        git.run_git(&["init"]).await.unwrap();
        git.run_git(&["config", "user.email", "test@example.com"])
            .await
            .unwrap();
        git.run_git(&["config", "user.name", "Test User"])
            .await
            .unwrap();
        std::fs::write(temp.path().join("README.md"), "# Init\n").unwrap();

        let svc = GitCommitService::new(&git);
        let first = svc
            .commit_if_dirty(
                "docs(readme): initialize readme",
                Some(&["README.md".into()]),
            )
            .await
            .unwrap();
        assert!(first.is_some());

        // Re-running with identical bytes must return Ok(None) rather than a fake hash string
        let clean = svc
            .commit_if_dirty("docs(readme): no-op", Some(&["README.md".into()]))
            .await
            .unwrap();
        assert_eq!(clean, None);
    }

    #[tokio::test]
    async fn test_minitest66_staged_files_and_landing_scope_inference() {
        // 1. Scope inference on scaffolded landing page (`src/components/landing/*` + root configs)
        let mut files = vec![
            ".gitignore".to_string(),
            "README.md".to_string(),
            "bun.lock".to_string(),
            "components.json".to_string(),
            "eslint.config.js".to_string(),
            "index.html".to_string(),
            "package.json".to_string(),
            "tsconfig.app.json".to_string(),
            "tsconfig.json".to_string(),
            "tsconfig.node.json".to_string(),
            "vite.config.ts".to_string(),
            "src/App.tsx".to_string(),
            "src/main.tsx".to_string(),
            "src/index.css".to_string(),
            "src/vite-env.d.ts".to_string(),
            "src/layouts/RootLayout.tsx".to_string(),
            "src/pages/HomePage.tsx".to_string(),
            "src/pages/NotFoundPage.tsx".to_string(),
            "src/lib/cn.ts".to_string(),
            "src/store/useAppStore.ts".to_string(),
            "src/components/ErrorBoundary.tsx".to_string(),
        ];
        for name in [
            "SiteNav.tsx",
            "HeroSection.tsx",
            "TerminalPreview.tsx",
            "PhilosophySection.tsx",
            "LightweightSection.tsx",
            "SwarmSection.tsx",
            "CouncilSection.tsx",
            "CodeIntelligenceSection.tsx",
            "SecuritySection.tsx",
            "BrowserHudSection.tsx",
            "ToolsSection.tsx",
            "SiteFooter.tsx",
        ] {
            files.push(format!("src/components/landing/{name}"));
        }
        assert_eq!(
            GitCommitService::infer_conventional_scope(&files),
            Some("landing".to_string())
        );
        let msg = GitCommitService::synthesize_turn_commit_message(
            &files,
            Some("minicode landing page"),
            Some("Create a premium, modern landing page for minicode, an autonomous AI coding agent."),
        );
        assert_eq!(
            msg.lines().next().unwrap(),
            "feat(landing): create a premium, modern landing page for minicode"
        );

        // 2. `stage_and_list_staged_files` on an unborn repo excludes gitignored files and deleted transients
        let temp = tempfile::tempdir().unwrap();
        let git = GitService::new(temp.path().to_path_buf());
        git.run_git(&["init"]).await.unwrap();
        std::fs::write(temp.path().join(".gitignore"), "minikit_docs/\n").unwrap();
        std::fs::create_dir_all(temp.path().join("minikit_docs/core")).unwrap();
        std::fs::write(
            temp.path().join("minikit_docs/core/todo.md"),
            "- [x] Done\n",
        )
        .unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src/App.tsx"),
            "export default function App() {}\n",
        )
        .unwrap();

        let svc = GitCommitService::new(&git);
        let candidate_paths = vec![
            ".gitignore".to_string(),
            "minikit_docs/core/todo.md".to_string(),
            "src/App.tsx".to_string(),
        ];
        let staged = svc
            .stage_and_list_staged_files(Some(&candidate_paths))
            .await
            .unwrap();
        assert!(staged.contains(&".gitignore".to_string()));
        assert!(staged.contains(&"src/App.tsx".to_string()));
        assert!(!staged.contains(&"minikit_docs/core/todo.md".to_string()));
    }

    #[test]
    fn test_synthesize_commit_message_avoids_unbalanced_parentheses() {
        let files = vec![
            "index.html".to_string(),
            "style.css".to_string(),
            "app.js".to_string(),
        ];
        let msg = GitCommitService::synthesize_turn_commit_message(
            &files,
            None,
            Some("Build a minimal dark-themed Pomodoro timer web app (index.html, style.css, app.js) with 25/5 presets, keyboard shortcuts, and visual state."),
        );
        let subject = msg.lines().next().unwrap();
        assert_eq!(
            subject,
            "feat(ui): build a minimal dark-themed Pomodoro timer web app"
        );
        assert_eq!(
            subject.chars().filter(|&c| c == '(').count(),
            subject.chars().filter(|&c| c == ')').count()
        );

        // Also verify direct fallback stripping when truncation lands inside brackets or backticks
        assert_eq!(
            GitCommitService::strip_unbalanced_trailing_delimiters(
                "add telemetry dashboard (metrics"
            ),
            "add telemetry dashboard"
        );
        assert_eq!(
            GitCommitService::strip_unbalanced_trailing_delimiters("fix parser [edge case"),
            "fix parser"
        );
        assert_eq!(
            GitCommitService::strip_unbalanced_trailing_delimiters("update `config.toml"),
            "update"
        );
    }
}
