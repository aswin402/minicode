use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Strip ANSI escape codes from output using the shared state machine.
pub fn strip_ansi(input: &str) -> String {
    crate::utils::strings::strip_ansi_codes(input)
}

/// Relative workspace path for declarative user-defined RTK output filters.
pub const RTK_FILTERS_TOML_REL_PATH: &str = ".minicode/rtk_filters.toml";

/// Token-efficient shell output filtering engine (inspired by `rtk-ai/rtk` — Rust Token Killer).
pub struct RtkFilter;

#[derive(Debug, Clone, PartialEq)]
pub struct FilterResult {
    pub content: String,
    pub original_lines: usize,
    pub filtered_lines: usize,
    pub saved_pct: f32,
    pub category: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
struct CustomTomlFilterFile {
    #[serde(default)]
    filters: Vec<CustomTomlFilterRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct CustomTomlFilterRule {
    #[allow(dead_code)]
    #[serde(default)]
    name: String,
    match_command: String,
    #[serde(default)]
    strip_lines_matching: Vec<String>,
    #[serde(default)]
    keep_lines_matching: Vec<String>,
    #[serde(default)]
    max_lines: Option<usize>,
    #[serde(default)]
    on_empty: Option<String>,
}

impl RtkFilter {
    /// Intercepts and condenses command output based on command type, using active context window.
    pub fn filter(command: &str, output: &str, exit_code: Option<i32>) -> FilterResult {
        let context_window = crate::context::donut::get_active_context_limit();
        Self::filter_for_context(command, output, exit_code, context_window)
    }

    /// Intercepts and condenses command output using a specific context window limit.
    pub fn filter_for_context(
        command: &str,
        output: &str,
        exit_code: Option<i32>,
        context_window: usize,
    ) -> FilterResult {
        Self::filter_internal(None, command, output, exit_code, context_window)
    }

    /// Workspace-aware filtering that additionally checks declarative `.minicode/rtk_filters.toml`
    /// rules before falling back to built-in native Rust RTK filters.
    pub fn filter_for_workspace(
        workspace_root: &Path,
        command: &str,
        output: &str,
        exit_code: Option<i32>,
        context_window: usize,
    ) -> FilterResult {
        Self::filter_internal(
            Some(workspace_root),
            command,
            output,
            exit_code,
            context_window,
        )
    }

    fn filter_internal(
        workspace_root: Option<&Path>,
        command: &str,
        output: &str,
        exit_code: Option<i32>,
        context_window: usize,
    ) -> FilterResult {
        let raw_clean = strip_ansi(output);
        let orig_lines = raw_clean.lines().count();

        let primary_lower = super::compactor::extract_primary_command(command).to_ascii_lowercase();

        // 1. Declarative project-custom TOML filters (`.minicode/rtk_filters.toml`)
        if let Some(ws) = workspace_root {
            if let Some(custom_out) =
                Self::apply_custom_toml_filters(ws, &primary_lower, &raw_clean)
            {
                let filt_lines = custom_out.lines().count();
                let saved_pct = if orig_lines > 0 {
                    ((orig_lines.saturating_sub(filt_lines)) as f32 / orig_lines as f32) * 100.0
                } else {
                    0.0
                };
                return FilterResult {
                    content: custom_out,
                    original_lines: orig_lines,
                    filtered_lines: filt_lines,
                    saved_pct,
                    category: "custom_toml",
                };
            }
        }

        if orig_lines <= 10 {
            return FilterResult {
                content: raw_clean,
                original_lines: orig_lines,
                filtered_lines: orig_lines,
                saved_pct: 0.0,
                category: "passthrough",
            };
        }

        let clean = Self::strip_command_progress_noise(&raw_clean);
        let generic_fallback = || {
            if context_window > 0 {
                Self::filter_generic_for_context(&clean, context_window)
            } else {
                Self::filter_generic(&clean, crate::constants::DONUT_STANDARD_THRESHOLD_LINES)
            }
        };

        let (mut filtered, category): (String, &'static str) =
            if primary_lower.contains("cargo test") || primary_lower.contains("cargo nextest") {
                (Self::filter_cargo_test(&clean, exit_code), "cargo")
            } else if primary_lower.contains("pytest") || primary_lower.contains("unittest") {
                (Self::filter_pytest(&clean, exit_code), "test")
            } else if primary_lower.contains("vitest") || primary_lower.contains("bun test") {
                (Self::filter_vitest_or_bun(&clean, exit_code), "test")
            } else if primary_lower.starts_with("git ")
                && primary_lower.contains(" status")
                && !primary_lower.contains("--porcelain")
                && !primary_lower.contains(" -s")
                && !primary_lower.contains("--short")
            {
                (Self::filter_git_status(&clean), "git")
            } else if primary_lower.starts_with("git ") && primary_lower.contains(" log") {
                (Self::filter_git_log(&clean), "git")
            } else if (primary_lower.contains("jest")
                || primary_lower.contains("npm test")
                || primary_lower.contains("yarn test")
                || primary_lower.contains("pnpm test"))
                && Self::has_jest_signature(&clean)
            {
                (Self::filter_jest(&clean, exit_code), "test")
            } else if Self::is_linter_or_typecheck_command(&primary_lower) {
                if let Some(grouped) = Self::filter_linter_grouped(&clean) {
                    (grouped, "lint")
                } else {
                    (generic_fallback(), "lint")
                }
            } else if Self::is_table_cli_command(&primary_lower) {
                (Self::filter_table_cli(&clean), "container")
            } else if Self::is_ls_or_find_command(&primary_lower) {
                (Self::filter_ls_or_find(&clean), "fs")
            } else {
                (generic_fallback(), "generic")
            };

        // Non-empty preservation invariant: a compaction filter must never erase 100% of non-empty stdout/stderr.
        if !clean.trim().is_empty() && filtered.trim().is_empty() {
            filtered = generic_fallback();
        }

        let filt_lines = filtered.lines().count();
        let saved_pct = if orig_lines > 0 {
            ((orig_lines.saturating_sub(filt_lines)) as f32 / orig_lines as f32) * 100.0
        } else {
            0.0
        };

        FilterResult {
            content: filtered,
            original_lines: orig_lines,
            filtered_lines: filt_lines,
            saved_pct,
            category,
        }
    }

    /// Evaluates optional project-custom TOML filter rules from `.minicode/rtk_filters.toml`.
    fn apply_custom_toml_filters(
        workspace_root: &Path,
        primary_lower: &str,
        raw_clean: &str,
    ) -> Option<String> {
        let path = workspace_root.join(RTK_FILTERS_TOML_REL_PATH);
        if !path.is_file() {
            return None;
        }
        let raw_toml = std::fs::read_to_string(&path).ok()?;
        let parsed = toml::from_str::<CustomTomlFilterFile>(&raw_toml).ok()?;

        let rule = parsed.filters.into_iter().find(|r| {
            let needle = r.match_command.trim().to_ascii_lowercase();
            !needle.is_empty() && primary_lower.contains(&needle)
        })?;

        let strip_res: Vec<regex::Regex> = rule
            .strip_lines_matching
            .iter()
            .filter_map(|p| regex::Regex::new(p).ok())
            .collect();
        let keep_res: Vec<regex::Regex> = rule
            .keep_lines_matching
            .iter()
            .filter_map(|p| regex::Regex::new(p).ok())
            .collect();

        let mut kept: Vec<&str> = Vec::new();
        for line in raw_clean.lines() {
            if !keep_res.is_empty() && !keep_res.iter().any(|re| re.is_match(line)) {
                continue;
            }
            if strip_res.iter().any(|re| re.is_match(line)) {
                continue;
            }
            kept.push(line);
        }

        if let Some(cap) = rule.max_lines {
            if cap > 0 && kept.len() > cap {
                let omitted = kept.len() - cap;
                let mut out: Vec<String> = kept.into_iter().take(cap).map(str::to_string).collect();
                out.push(format!(
                    "... (+{omitted} lines omitted by rtk_filters.toml)"
                ));
                return Some(out.join("\n"));
            }
        }

        if kept.is_empty() {
            if let Some(empty_msg) = rule.on_empty {
                return Some(empty_msg);
            }
            return None;
        }

        Some(kept.join("\n"))
    }

    fn is_linter_or_typecheck_command(primary_lower: &str) -> bool {
        let tokens: Vec<&str> = primary_lower.split_whitespace().collect();
        let Some(&first) = tokens.first() else {
            return false;
        };
        matches!(first, "tsc" | "eslint" | "ruff" | "mypy" | "biome")
            || primary_lower.contains("npx tsc")
            || primary_lower.contains("npx eslint")
            || primary_lower.contains("bunx tsc")
            || primary_lower.contains("bunx eslint")
            || primary_lower.contains("bunx biome")
            || primary_lower.contains("npm run lint")
            || primary_lower.contains("bun run lint")
            || primary_lower.contains("cargo clippy")
    }

    fn is_table_cli_command(primary_lower: &str) -> bool {
        primary_lower.starts_with("docker ps")
            || primary_lower.starts_with("docker images")
            || primary_lower.starts_with("kubectl get")
            || primary_lower.starts_with("gh pr list")
            || primary_lower.starts_with("gh issue list")
            || primary_lower.starts_with("gh run list")
    }

    fn is_ls_or_find_command(primary_lower: &str) -> bool {
        let first = primary_lower.split_whitespace().next().unwrap_or("");
        matches!(first, "ls" | "find" | "tree")
    }

    /// Compresses verbose `git status` prose into a compact grouped summary while preserving
    /// branch info, conflicts, and up to 20 paths per section.
    pub fn filter_git_status(output: &str) -> String {
        enum Section {
            None,
            Staged,
            Unstaged,
            Unmerged,
            Untracked,
        }

        let mut branch_line: Option<String> = None;
        let mut ahead_behind: Option<String> = None;
        let mut staged: Vec<String> = Vec::new();
        let mut unstaged: Vec<String> = Vec::new();
        let mut unmerged: Vec<String> = Vec::new();
        let mut untracked: Vec<String> = Vec::new();
        let mut section = Section::None;

        for raw_line in output.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with("(use \"git ") {
                continue;
            }
            if trimmed.starts_with("On branch ") || trimmed.starts_with("HEAD detached") {
                branch_line = Some(trimmed.to_string());
                continue;
            }
            if trimmed.starts_with("Your branch is ") {
                ahead_behind = Some(trimmed.to_string());
                continue;
            }
            if trimmed.starts_with("Changes to be committed:") {
                section = Section::Staged;
                continue;
            }
            if trimmed.starts_with("Changes not staged for commit:") {
                section = Section::Unstaged;
                continue;
            }
            if trimmed.starts_with("Unmerged paths:") {
                section = Section::Unmerged;
                continue;
            }
            if trimmed.starts_with("Untracked files:") {
                section = Section::Untracked;
                continue;
            }
            if trimmed.starts_with("no changes added")
                || trimmed.starts_with("nothing added to commit")
                || trimmed.starts_with("nothing to commit")
            {
                continue;
            }

            let entry = trimmed
                .replace("modified:   ", "M:")
                .replace("new file:   ", "A:")
                .replace("deleted:    ", "D:")
                .replace("renamed:    ", "R:")
                .replace("both modified:   ", "UU:");

            match section {
                Section::Staged => staged.push(entry),
                Section::Unstaged => unstaged.push(entry),
                Section::Unmerged => unmerged.push(entry),
                Section::Untracked => untracked.push(entry),
                Section::None => {}
            }
        }

        if branch_line.is_none()
            && staged.is_empty()
            && unstaged.is_empty()
            && unmerged.is_empty()
            && untracked.is_empty()
        {
            return output.to_string();
        }

        let fmt_bucket = |label: &str, items: &[String], limit: usize| -> Option<String> {
            if items.is_empty() {
                return None;
            }
            let shown: Vec<&str> = items.iter().take(limit).map(String::as_str).collect();
            let suffix = if items.len() > limit {
                format!(" (+{} more)", items.len() - limit)
            } else {
                String::new()
            };
            Some(format!(
                "{label} ({}): {}{suffix}",
                items.len(),
                shown.join(", ")
            ))
        };

        let mut parts = Vec::new();
        if let Some(b) = branch_line {
            if let Some(ab) = ahead_behind {
                parts.push(format!("{b} ({ab})"));
            } else {
                parts.push(b);
            }
        }
        if let Some(s) = fmt_bucket("Conflicts", &unmerged, 30) {
            parts.push(s);
        }
        if let Some(s) = fmt_bucket("Staged", &staged, 20) {
            parts.push(s);
        }
        if let Some(s) = fmt_bucket("Modified", &unstaged, 20) {
            parts.push(s);
        }
        if let Some(s) = fmt_bucket("Untracked", &untracked, 15) {
            parts.push(s);
        }

        parts.join("\n")
    }

    /// Groups repetitive linter/typechecker diagnostics (`tsc`, `eslint`, `ruff`, `mypy`, `biome`,
    /// `cargo clippy`) by rule or error code while keeping every unique rule and sample locations.
    pub fn filter_linter_grouped(output: &str) -> Option<String> {
        let mut grouped: BTreeMap<String, (usize, String, Vec<String>)> = BTreeMap::new();
        let mut total_diags = 0usize;
        let mut summary_lines: Vec<String> = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("Found ")
                || trimmed.starts_with("✖ ")
                || trimmed.starts_with("error: could not compile")
                || trimmed.starts_with("Finished ")
            {
                summary_lines.push(trimmed.to_string());
                continue;
            }

            if let Some((code, loc, sample)) = Self::parse_diagnostic_line(trimmed) {
                total_diags += 1;
                let entry = grouped
                    .entry(code)
                    .or_insert_with(|| (0, sample, Vec::new()));
                entry.0 += 1;
                if entry.2.len() < 8 && !loc.is_empty() && !entry.2.contains(&loc) {
                    entry.2.push(loc);
                }
            }
        }

        // Only apply grouped view when there are enough diagnostics to produce real compression
        if total_diags < 6 || grouped.len() >= total_diags {
            return None;
        }

        let mut rows: Vec<(String, usize, String, Vec<String>)> = grouped
            .into_iter()
            .map(|(code, (count, sample, locs))| (code, count, sample, locs))
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut out = vec![format!(
            "Grouped {} diagnostic(s) across {} unique rule(s):",
            total_diags,
            rows.len()
        )];
        for (code, count, sample, locs) in rows.into_iter().take(25) {
            let loc_str = if locs.is_empty() {
                String::new()
            } else {
                let more = count.saturating_sub(locs.len());
                if more > 0 {
                    format!(" [{} (+{} more)]", locs.join(", "), more)
                } else {
                    format!(" [{}]", locs.join(", "))
                }
            };
            out.push(format!("- {code} ({count}x): {sample}{loc_str}"));
        }
        out.extend(summary_lines);
        Some(out.join("\n"))
    }

    /// Parses a single diagnostic line from `tsc`, `ruff`, `mypy`, `eslint`, or `rustc`/`clippy`
    /// into `(rule_or_code, location, concise_message)`.
    fn parse_diagnostic_line(line: &str) -> Option<(String, String, String)> {
        // TypeScript: `src/app.ts(12,5): error TS2345: Argument of type ...`
        if let Some(ts_pos) = line.find("TS") {
            let after = &line[ts_pos + 2..];
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            if digits.len() >= 4 {
                let code = format!("TS{digits}");
                let loc = line
                    .split_once(": ")
                    .map(|(l, _)| l.trim().to_string())
                    .unwrap_or_default();
                let msg = line[ts_pos + 2 + digits.len()..]
                    .trim_start_matches(':')
                    .trim();
                return Some((
                    code,
                    loc,
                    crate::utils::strings::truncate_ellipsis(msg, 100),
                ));
            }
        }

        // Rust / Clippy: `error[E0308]: mismatched types` or `warning: ... #[warn(clippy::foo)]`
        if let Some(start) = line.find("error[E") {
            if let Some(end_rel) = line[start..].find(']') {
                let code = line[start + 6..start + end_rel].to_string();
                let msg = line[start + end_rel + 1..].trim_start_matches(':').trim();
                return Some((
                    code,
                    String::new(),
                    crate::utils::strings::truncate_ellipsis(msg, 100),
                ));
            }
        }

        // Ruff / Flake8 / Mypy: `src/a.py:12:5: E501 Line too long` or `src/a.py:12: error: ... [arg-type]`
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 3 && parts[1].trim().chars().all(|c| c.is_ascii_digit()) {
            let loc = format!("{}:{}", parts[0].trim(), parts[1].trim());
            let rest = if parts.len() == 4 && parts[2].trim().chars().all(|c| c.is_ascii_digit()) {
                parts[3].trim()
            } else {
                parts[2].trim()
            };
            // Trailing bracket rule e.g. `[arg-type]` or `[no-unused-vars]`
            if let Some(open) = rest.rfind('[') {
                if rest.ends_with(']') && open + 2 < rest.len() {
                    let code = rest[open + 1..rest.len() - 1].trim().to_string();
                    let msg = rest[..open].trim();
                    if !code.is_empty() && !code.contains(' ') {
                        return Some((
                            code,
                            loc,
                            crate::utils::strings::truncate_ellipsis(msg, 100),
                        ));
                    }
                }
            }
            // Leading short rule code e.g. `E501`, `F401`, `W291`, `B006`
            if let Some((tok, msg)) = rest.split_once(' ') {
                let is_ruff_code = tok.len() >= 2
                    && tok.len() <= 8
                    && tok.as_bytes()[0].is_ascii_uppercase()
                    && tok.bytes().any(|b| b.is_ascii_digit());
                if is_ruff_code {
                    return Some((
                        tok.to_string(),
                        loc,
                        crate::utils::strings::truncate_ellipsis(msg.trim(), 100),
                    ));
                }
            }
        }

        // ESLint compact/stylish line: `12:5  error  'x' is assigned a value  no-unused-vars`
        if (line.contains("  error  ") || line.contains("  warning  "))
            && line
                .split_whitespace()
                .next()
                .is_some_and(|t| t.contains(':'))
        {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() >= 4 {
                let loc = tokens[0].to_string();
                let rule = tokens[tokens.len() - 1].to_string();
                let msg = tokens[2..tokens.len() - 1].join(" ");
                return Some((
                    rule,
                    loc,
                    crate::utils::strings::truncate_ellipsis(&msg, 100),
                ));
            }
        }

        None
    }

    /// Filters `vitest` and `bun test` outputs, collapsing passing specs on success and isolating
    /// failing assertions/summaries on failure.
    pub fn filter_vitest_or_bun(output: &str, exit_code: Option<i32>) -> String {
        if exit_code == Some(0) {
            let mut summary = Vec::new();
            for line in output.lines() {
                let t = line.trim();
                if t.starts_with("Test Files")
                    || t.starts_with("Tests ")
                    || t.starts_with("Duration")
                    || t.contains(" pass")
                    || t.contains(" fail")
                    || t.starts_with("Ran ")
                {
                    summary.push(line);
                }
            }
            if !summary.is_empty() {
                return format!("✔ All tests passed:\n{}", summary.join("\n"));
            }
        }

        let mut kept = Vec::new();
        for line in output.lines() {
            let t = line.trim();
            if (t.starts_with("✓ ") || t.starts_with("(pass) ") || t.starts_with("PASS "))
                && !t.contains("FAIL")
                && !t.contains("fail")
            {
                continue;
            }
            kept.push(line);
        }
        if kept.is_empty() {
            output.to_string()
        } else {
            kept.join("\n")
        }
    }

    /// Strips high-noise build/dependency/VCS paths from `ls`, `find`, and `tree` listings.
    pub fn filter_ls_or_find(output: &str) -> String {
        const IGNORED_SEGMENTS: &[&str] = &[
            "/.git/",
            "/node_modules/",
            "/target/",
            "/__pycache__/",
            "/.minicode/",
            "/.venv/",
            "/.pytest_cache/",
        ];
        let mut kept = Vec::new();
        let mut skipped = 0usize;
        for line in output.lines() {
            let norm = format!("/{}/", line.trim().trim_matches('/'));
            if IGNORED_SEGMENTS.iter().any(|seg| norm.contains(seg)) {
                skipped += 1;
                continue;
            }
            kept.push(line);
        }
        let max_keep = 80usize;
        let truncated = kept.len().saturating_sub(max_keep);
        let mut out: Vec<String> = kept
            .into_iter()
            .take(max_keep)
            .map(str::to_string)
            .collect();
        if skipped > 0 || truncated > 0 {
            out.push(format!(
                "[... filtered {} noise path(s), {} extra line(s) omitted ...]",
                skipped, truncated
            ));
        }
        out.join("\n")
    }

    /// Compresses wide whitespace-padded CLI tables (`docker ps`, `kubectl get`, `gh pr list`)
    /// while preserving header + failing/unhealthy rows.
    pub fn filter_table_cli(output: &str) -> String {
        let mut lines = output.lines();
        let Some(header) = lines.next() else {
            return String::new();
        };
        let compact_row = |row: &str| -> String {
            row.split("  ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" | ")
        };

        let mut out = vec![compact_row(header)];
        let mut normal_rows = Vec::new();
        let mut alert_rows = Vec::new();

        for row in lines {
            if row.trim().is_empty() {
                continue;
            }
            let lower = row.to_ascii_lowercase();
            let compact = compact_row(row);
            if lower.contains("error")
                || lower.contains("crashloop")
                || lower.contains("exited")
                || lower.contains("failed")
                || lower.contains("unhealthy")
            {
                alert_rows.push(compact);
            } else {
                normal_rows.push(compact);
            }
        }

        out.extend(alert_rows);
        let max_normal = 25usize;
        let omitted = normal_rows.len().saturating_sub(max_normal);
        out.extend(normal_rows.into_iter().take(max_normal));
        if omitted > 0 {
            out.push(format!("... (+{omitted} more rows)"));
        }
        out.join("\n")
    }

    /// Filters cargo test output, preserving failed assertions, stack traces, and test summary
    pub fn filter_cargo_test(output: &str, exit_code: Option<i32>) -> String {
        let is_success = exit_code == Some(0);
        if is_success {
            let mut summary_lines = Vec::new();
            for line in output.lines() {
                if line.contains("test result:")
                    || line.contains("Doc-tests")
                    || line.contains("running ")
                {
                    summary_lines.push(line);
                }
            }
            if summary_lines.is_empty() {
                output.lines().take(15).collect::<Vec<_>>().join("\n")
            } else {
                format!(
                    "✔ All tests passed successfully:\n{}",
                    summary_lines.join("\n")
                )
            }
        } else {
            let mut out = Vec::new();
            let mut in_failures = false;
            let mut after_panic = false;
            let mut summary = String::new();

            for line in output.lines() {
                let trimmed = line.trim();
                if line.contains("failures:") {
                    in_failures = true;
                    out.push(line);
                    continue;
                }
                if line.starts_with("test result:") {
                    summary = line.to_string();
                    in_failures = false;
                }
                let is_panic = line.contains("panicked at");
                let is_diff = trimmed.starts_with("left:")
                    || trimmed.starts_with("right:")
                    || line.contains("assertion ");
                if in_failures || line.contains("FAILED") || is_panic || after_panic || is_diff {
                    out.push(line);
                }
                after_panic = is_panic;
            }

            if out.is_empty() {
                output
                    .lines()
                    .rev()
                    .take(30)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                if !summary.is_empty() {
                    out.push("");
                    out.push(&summary);
                }
                out.join("\n")
            }
        }
    }

    /// Filters pytest output, isolating failures, errors, and summary line
    pub fn filter_pytest(output: &str, exit_code: Option<i32>) -> String {
        if exit_code == Some(0) {
            let last_lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
            last_lines
                .iter()
                .rev()
                .take(3)
                .rev()
                .copied()
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            let mut failures = Vec::new();
            let mut capture = false;
            for line in output.lines() {
                if line.starts_with("FAILURES")
                    || line.starts_with("ERRORS")
                    || line.contains("FAILED ")
                    || line.starts_with("FAIL: ")
                    || line.starts_with("ERROR: ")
                {
                    capture = true;
                }
                if capture {
                    failures.push(line);
                }
                if line.starts_with("=====") && line.contains("failed") {
                    capture = false;
                }
            }
            if failures.is_empty() {
                output
                    .lines()
                    .rev()
                    .take(25)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                failures.join("\n")
            }
        }
    }

    /// Filters git log into clean one-line summaries
    pub fn filter_git_log(output: &str) -> String {
        let mut lines = Vec::new();
        for line in output.lines().take(30) {
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
        lines.join("\n")
    }

    /// Structural output probe verifying Jest summary/suite output rather than assuming every `npm test` uses Jest.
    fn has_jest_signature(output: &str) -> bool {
        output.contains("Test Suites:")
            || output.contains("Ran all test suites")
            || (output.contains("Tests:")
                && (output.contains("Snapshots:") || output.contains("Time:")))
    }

    /// Filters jest outputs
    pub fn filter_jest(output: &str, exit_code: Option<i32>) -> String {
        if exit_code == Some(0) {
            output
                .lines()
                .filter(|l| l.contains("Tests:") || l.contains("Snapshots:") || l.contains("Time:"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            output
                .lines()
                .filter(|l| {
                    l.contains("●")
                        || l.contains("FAIL")
                        || l.contains("Error:")
                        || l.contains("Tests:")
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    /// Generic head + tail truncation via SmartDonutTruncator preserving critical error/diagnostic lines
    pub fn filter_generic(output: &str, max_lines: usize) -> String {
        let head = max_lines.saturating_mul(3) / 10;
        let tail = max_lines.saturating_mul(5) / 10;
        let max_errors = max_lines.saturating_mul(3) / 10;
        crate::context::budget::donut::SmartDonutTruncator::truncate_custom(
            output,
            max_lines,
            head.max(10),
            tail.max(15),
            max_errors.max(10),
        )
        .content
    }

    /// Generic truncation via SmartDonutTruncator dynamically scaling to model context window
    pub fn filter_generic_for_context(output: &str, context_window: usize) -> String {
        crate::context::budget::donut::SmartDonutTruncator::truncate_for_context(
            output,
            context_window,
        )
        .content
    }

    /// Strips high-token CLI progress noise (`git` object transfer counters, `pip` requirement-satisfied
    /// lines, `npm notice`, and Braille spinners) while keeping all warnings/errors (`headroom-mcp` `cmd_filters.rs` parity).
    pub fn strip_command_progress_noise(output: &str) -> String {
        let mut out = Vec::new();
        let mut deprecated_count = 0usize;

        for line in output.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_ascii_lowercase();

            if trimmed.starts_with("Enumerating objects:")
                || trimmed.starts_with("Counting objects:")
                || trimmed.starts_with("Compressing objects:")
                || trimmed.starts_with("Writing objects:")
                || trimmed.starts_with("Resolving deltas:")
                || trimmed.starts_with("remote: Enumerating objects:")
                || trimmed.starts_with("remote: Counting objects:")
                || trimmed.starts_with("remote: Compressing objects:")
                || trimmed.starts_with("remote: Resolving deltas:")
                || trimmed.starts_with("Requirement already satisfied:")
                || lower.starts_with("npm notice")
            {
                continue;
            }

            if lower.contains("warn deprecated") || lower.contains("warning deprecated") {
                if deprecated_count < 3 {
                    deprecated_count += 1;
                    out.push(line);
                }
                continue;
            }

            if trimmed
                .chars()
                .any(|c| matches!(c, '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏'))
                && !lower.contains("error")
                && !lower.contains("fail")
            {
                continue;
            }

            out.push(line);
        }

        if out.is_empty() {
            output.to_string()
        } else {
            out.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtk_filter_cargo_test_success() {
        let raw = r#"
running 10 tests
test tools::test_1 ... ok
test tools::test_2 ... ok
test tools::test_3 ... ok
test tools::test_4 ... ok
test tools::test_5 ... ok
test tools::test_6 ... ok
test tools::test_7 ... ok
test tools::test_8 ... ok
test tools::test_9 ... ok
test tools::test_10 ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
"#;

        let res = RtkFilter::filter("nice -n 19 cargo test -j 1", raw, Some(0));
        assert_eq!(res.category, "cargo");
        assert!(res.content.contains("All tests passed"));
        assert!(res.content.contains("test result: ok. 10 passed"));
        assert!(res.saved_pct > 0.0);
    }

    #[test]
    fn test_rtk_filter_cargo_test_failure() {
        let raw = r#"
running 3 tests
test test_ok_1 ... ok
test test_fail_2 ... FAILED
test test_ok_3 ... ok

failures:

---- test_fail_2 stdout ----
thread 'test_fail_2' panicked at src/lib.rs:42:9:
assertion `left == right` failed
  left: 1
 right: 2

failures:
    test_fail_2

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;

        let res = RtkFilter::filter("cargo test", raw, Some(101));
        assert!(res.content.contains("failures:"));
        assert!(res.content.contains("test_fail_2"));
        assert!(res.content.contains("assertion `left == right` failed"));
    }

    #[test]
    fn test_rtk_filter_npm_test_non_jest_tap_and_chained_commands_never_blank() {
        let tap_out = "\
TAP version 13
# Subtest: TokenBucketStore: consumes up to capacity then rejects
ok 1 - TokenBucketStore: consumes up to capacity then rejects
ok 2 - TokenBucketStore: refills over elapsed interval
ok 3 - SlidingWindowMetrics: p50/p95/p99
ok 4 - HTTP: 429 Retry-After header
ok 5 - HTTP: malformed JSON 400
ok 6 - CLI: --simulate report
1..6
# tests 6
# suites 0
# pass 6
# fail 0
# duration_ms 142.5
";
        let res = RtkFilter::filter("cd /workspace && npm test", tap_out, Some(0));
        assert!(
            !res.content.trim().is_empty(),
            "non-Jest npm test output must never be wiped to empty string"
        );
        assert!(res.content.contains("# pass 6"));

        let chained = "cd /workspace && npm test > .minicode/tmp/final.log 2>&1; echo \"exit=$?\"; tail -12 .minicode/tmp/final.log";
        let chained_res = RtkFilter::filter(chained, tap_out, Some(0));
        assert!(!chained_res.content.trim().is_empty());
        assert!(chained_res.content.contains("# tests 6"));
    }

    #[test]
    fn test_rtk_filter_git_status_compacts_verbose_prose() {
        let raw = "\
On branch main
Your branch is ahead of 'origin/main' by 1 commit.
  (use \"git push\" to publish your local commits)

Changes to be committed:
  (use \"git restore --staged <file>...\" to unstage)
\tmodified:   src/lib.rs
\tnew file:   src/new.rs

Changes not staged for commit:
  (use \"git add <file>...\" to update what will be committed)
  (use \"git restore <file>...\" to discard changes in working directory)
\tmodified:   src/main.rs
\tmodified:   Cargo.toml

Untracked files:
  (use \"git add <file>...\" to include in what will be committed)
\tnotes.txt
";
        let res = RtkFilter::filter("git status", raw, Some(0));
        assert_eq!(res.category, "git");
        assert!(res.content.contains("On branch main"));
        assert!(res
            .content
            .contains("Staged (2): M:src/lib.rs, A:src/new.rs"));
        assert!(res
            .content
            .contains("Modified (2): M:src/main.rs, M:Cargo.toml"));
        assert!(res.content.contains("Untracked (1): notes.txt"));
        assert!(!res.content.contains("use \"git add"));
        assert!(res.saved_pct > 50.0);
    }

    #[test]
    fn test_rtk_filter_linter_groups_repetitive_tsc_and_ruff_codes() {
        let mut tsc = String::new();
        for i in 1..=8 {
            tsc.push_str(&format!(
                "src/file_{i}.ts({i},5): error TS2345: Argument of type 'string' is not assignable to parameter of type 'number'.\n"
            ));
        }
        for i in 1..=4 {
            tsc.push_str(&format!(
                "src/other_{i}.ts({i},2): error TS2322: Type 'null' is not assignable to type 'string'.\n"
            ));
        }
        let res = RtkFilter::filter("npx tsc --noEmit", &tsc, Some(2));
        assert_eq!(res.category, "lint");
        assert!(res.content.contains("TS2345 (8x)"));
        assert!(res.content.contains("TS2322 (4x)"));
        assert!(res.saved_pct > 50.0);
    }

    #[test]
    fn test_rtk_filter_custom_toml_workspace_rule() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_dir = tmp.path().join(".minicode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("rtk_filters.toml"),
            r#"
[[filters]]
name = "terraform-plan"
match_command = "terraform plan"
strip_lines_matching = ["^Refreshing state\\.\\.\\.", "^\\s*$"]
max_lines = 5
"#,
        )
        .unwrap();

        let raw = "\
Refreshing state... aws_s3_bucket.a
Refreshing state... aws_s3_bucket.b
Refreshing state... aws_s3_bucket.c
Plan: 1 to add, 0 to change, 0 to destroy.
";
        let res =
            RtkFilter::filter_for_workspace(tmp.path(), "terraform plan", raw, Some(0), 64_000);
        assert_eq!(res.category, "custom_toml");
        assert_eq!(res.content, "Plan: 1 to add, 0 to change, 0 to destroy.");
        assert!(res.saved_pct >= 70.0);
    }

    #[test]
    fn test_rtk_filter_vitest_table_and_find_noise() {
        let find_out = "\
./src/main.rs
./.git/objects/ab/1234
./node_modules/react/index.js
./target/debug/deps/foo.rlib
./__pycache__/billing.pyc
./src/lib.rs
./src/a.rs
./src/b.rs
./src/c.rs
./src/d.rs
./src/e.rs
./src/f.rs
";
        let fres = RtkFilter::filter("find . -maxdepth 4", find_out, Some(0));
        assert_eq!(fres.category, "fs");
        assert!(fres.content.contains("./src/main.rs"));
        assert!(!fres.content.contains("node_modules"));
        assert!(!fres.content.contains("__pycache__"));

        let docker_out = "\
CONTAINER ID   IMAGE          COMMAND   CREATED      STATUS                      PORTS     NAMES
a1b2c3         nginx:latest   nginx     2 hours ago  Up 2 hours                  80/tcp    web
d4e5f6         api:latest     node      1 hour ago   Exited (1) 5 minutes ago              api
111111         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache1
222222         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache2
333333         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache3
444444         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache4
555555         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache5
666666         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache6
777777         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache7
888888         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache8
999999         redis:7        redis     2 hours ago  Up 2 hours                  6379/tcp  cache9
";
        let dres = RtkFilter::filter("docker ps -a", docker_out, Some(0));
        assert_eq!(dres.category, "container");
        assert!(dres.content.contains("Exited (1)"));
        assert!(dres.content.contains(" | "));
    }
}
