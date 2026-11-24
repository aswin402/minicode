//! Deterministic multi-gate file filtering, secret-path protection, built-in language review rules,
//! and repo-local `.minicode/review_rules.json` / `.opencodereview/rule.json` resolution.
//!
//! Architecture & rule discipline inspired by `alibaba/open-code-review` (Apache-2.0).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Default repository rule configuration locations checked in precedence order.
pub const MINICODE_REVIEW_RULES_PATH: &str = ".minicode/review_rules.json";
pub const OPEN_CODE_REVIEW_RULES_PATH: &str = ".opencodereview/rule.json";

/// Reason a changed/workspace path was excluded before reaching the LLM reviewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    SecretPath,
    Lockfile,
    GeneratedOrVendor,
    BinaryOrMedia,
    CustomExcludeRule,
}

impl ExclusionReason {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::SecretPath => "secret/credential path blocked",
            Self::Lockfile => "lockfile skipped",
            Self::GeneratedOrVendor => "vendor/build/generated path skipped",
            Self::BinaryOrMedia => "binary/media asset skipped",
            Self::CustomExcludeRule => "excluded by repo review rule",
        }
    }
}

/// Record of an excluded file so `--preview` and audit logs stay 100% transparent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedFile {
    pub path: String,
    pub reason: ExclusionReason,
}

/// Result of running the deterministic multi-gate file selector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileSelectionReport {
    pub reviewable_files: Vec<String>,
    pub excluded_files: Vec<ExcludedFile>,
}

/// Drop-in schema compatible with both `.minicode/review_rules.json` and `.opencodereview/rule.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoRuleConfig {
    #[serde(default)]
    pub rules: Vec<RepoPathRule>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// A single path-scoped review rule entry in `review_rules.json` / `rule.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoPathRule {
    /// Exact relative path or glob pattern (e.g. `src/agent/**/*.rs` or `Cargo.toml`).
    pub path: String,
    /// Review instructions enforced when matching files are touched.
    pub rule: String,
    /// When `true` (default), merges with the built-in language rule; when `false`, replaces it.
    #[serde(default = "default_merge_system_rule")]
    pub merge_system_rule: bool,
}

fn default_merge_system_rule() -> bool {
    true
}

/// A deduplicated review rule group mapped to the exact subset of files it governs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRuleGroup {
    pub rule_id: String,
    pub source: String,
    pub files: Vec<String>,
    pub rule_text: String,
}

/// Deterministic file gate + language & repo-custom review rule resolver.
pub struct ReviewRuleEngine {
    config: RepoRuleConfig,
    config_source: Option<String>,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
}

impl ReviewRuleEngine {
    /// Loads repository rules from an explicit path, `.minicode/review_rules.json`, or
    /// `.opencodereview/rule.json` (in that precedence order). Falls back cleanly to built-in
    /// language rules if no repo rule file exists or if JSON is malformed.
    #[must_use]
    pub fn load(workspace_root: &Path, explicit_rule_path: Option<&Path>) -> Self {
        let candidates: Vec<(std::path::PathBuf, String)> = match explicit_rule_path {
            Some(p) => {
                let full = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    workspace_root.join(p)
                };
                vec![(full, p.display().to_string())]
            }
            None => vec![
                (
                    workspace_root.join(MINICODE_REVIEW_RULES_PATH),
                    MINICODE_REVIEW_RULES_PATH.to_string(),
                ),
                (
                    workspace_root.join(OPEN_CODE_REVIEW_RULES_PATH),
                    OPEN_CODE_REVIEW_RULES_PATH.to_string(),
                ),
            ],
        };

        let mut config = RepoRuleConfig::default();
        let mut config_source = None;
        for (path, label) in candidates {
            if !path.is_file() {
                continue;
            }
            match std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<RepoRuleConfig>(&raw).ok())
            {
                Some(parsed) => {
                    config = parsed;
                    config_source = Some(label);
                    break;
                }
                None => {
                    tracing::warn!(
                        "Failed to parse review rules config at {}; falling back to built-in rules",
                        path.display()
                    );
                }
            }
        }

        let include_patterns = normalize_patterns(&config.include);
        let exclude_patterns = normalize_patterns(&config.exclude);

        Self {
            config,
            config_source,
            include_patterns,
            exclude_patterns,
        }
    }

    #[must_use]
    pub fn config_source(&self) -> Option<&str> {
        self.config_source.as_deref()
    }

    /// Returns `true` when `rel_path` matches a credential, private key, or secret config path
    /// that must NEVER be sent to an LLM reviewer.
    #[must_use]
    pub fn is_secret_path(rel_path: &str) -> bool {
        let norm = rel_path.replace('\\', "/").to_ascii_lowercase();
        let file_name = norm.rsplit('/').next().unwrap_or(norm.as_str());

        // Safe example/template env files remain reviewable
        let is_template_env = file_name.ends_with(".example")
            || file_name.ends_with(".sample")
            || file_name.ends_with(".template")
            || file_name.ends_with(".dist");

        if !is_template_env
            && (file_name == ".env"
                || file_name.starts_with(".env.")
                || file_name.ends_with(".env"))
        {
            return true;
        }

        const SECRET_EXACT_NAMES: &[&str] = &[
            "id_rsa",
            "id_dsa",
            "id_ecdsa",
            "id_ed25519",
            ".npmrc",
            ".pypirc",
            ".netrc",
            ".htpasswd",
            "kubeconfig",
            "credentials",
            "credentials.json",
            "service-account.json",
            "service_account.json",
            "secrets.json",
            "secrets.yaml",
            "secrets.yml",
            "secrets.toml",
            ".secrets",
            "vault.json",
            "master.key",
        ];
        if SECRET_EXACT_NAMES.contains(&file_name) {
            return true;
        }

        const SECRET_EXTS: &[&str] = &[
            ".pem",
            ".key",
            ".p12",
            ".pfx",
            ".jks",
            ".keystore",
            ".kdbx",
            ".ppk",
            ".der",
            ".crt",
            ".cer",
        ];
        if SECRET_EXTS.iter().any(|ext| file_name.ends_with(ext)) {
            return true;
        }

        if norm.contains("/.ssh/")
            || norm.starts_with(".ssh/")
            || norm.contains("/.aws/")
            || norm.starts_with(".aws/")
            || norm.contains("/.gnupg/")
        {
            return true;
        }

        false
    }

    /// Classifies why a path should be deterministically skipped before LLM review (if at all).
    #[must_use]
    pub fn classify_exclusion(&self, rel_path: &str) -> Option<ExclusionReason> {
        let norm = rel_path.replace('\\', "/");
        let lower = norm.to_ascii_lowercase();
        let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());

        // 1. Secret / credential paths are ALWAYS blocked unconditionally (even if included)
        if Self::is_secret_path(&norm) {
            return Some(ExclusionReason::SecretPath);
        }

        // 2. Vendor, build outputs, tool metadata, and minified artifacts
        const IGNORED_PREFIXES: &[&str] = &[
            "node_modules/",
            "target/",
            "dist/",
            "build/",
            ".next/",
            ".nuxt/",
            ".svelte-kit/",
            "vendor/",
            ".git/",
            ".minicode/",
            "minikit_docs/packages/",
            "onpkg_docs/packages/",
            "__pycache__/",
            ".venv/",
            "venv/",
            "coverage/",
        ];
        if IGNORED_PREFIXES
            .iter()
            .any(|p| lower.starts_with(p) || lower.contains(&format!("/{p}")))
        {
            return Some(ExclusionReason::GeneratedOrVendor);
        }
        if lower.ends_with(".min.js")
            || lower.ends_with(".min.css")
            || lower.ends_with(".bundle.js")
            || lower.ends_with(".map")
            || lower.ends_with(".d.ts.map")
        {
            return Some(ExclusionReason::GeneratedOrVendor);
        }

        // 3. Machine-generated lockfiles
        const LOCKFILES: &[&str] = &[
            "cargo.lock",
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "bun.lock",
            "bun.lockb",
            "poetry.lock",
            "uv.lock",
            "gemfile.lock",
            "composer.lock",
            "go.sum",
            "flake.lock",
            "pubspec.lock",
        ];
        if LOCKFILES.contains(&file_name) {
            return Some(ExclusionReason::Lockfile);
        }

        // 4. Binary / media / font / archive extensions
        const BINARY_EXTS: &[&str] = &[
            ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".bmp", ".tiff", ".avif", ".mp3",
            ".mp4", ".wav", ".webm", ".ogg", ".mov", ".woff", ".woff2", ".ttf", ".otf", ".eot",
            ".pdf", ".zip", ".tar", ".gz", ".tgz", ".xz", ".bz2", ".7z", ".wasm", ".bin", ".exe",
            ".dll", ".so", ".dylib", ".a", ".o", ".rlib", ".db", ".sqlite", ".sqlite3",
        ];
        if BINARY_EXTS.iter().any(|ext| file_name.ends_with(ext)) {
            return Some(ExclusionReason::BinaryOrMedia);
        }

        // 5. User-configured exclude / include globs from review_rules.json / rule.json
        if !self.exclude_patterns.is_empty()
            && self
                .exclude_patterns
                .iter()
                .any(|pat| rule_path_matches(pat, &norm))
        {
            return Some(ExclusionReason::CustomExcludeRule);
        }
        if !self.include_patterns.is_empty()
            && !self
                .include_patterns
                .iter()
                .any(|pat| rule_path_matches(pat, &norm))
        {
            return Some(ExclusionReason::CustomExcludeRule);
        }

        None
    }

    /// Runs the deterministic multi-gate file filter over candidate relative paths.
    #[must_use]
    pub fn filter_files(&self, candidates: &[String]) -> FileSelectionReport {
        let mut report = FileSelectionReport::default();
        let mut seen = HashSet::new();

        for raw in candidates {
            let norm = raw.trim().replace('\\', "/");
            if norm.is_empty() || !seen.insert(norm.clone()) {
                continue;
            }
            match self.classify_exclusion(&norm) {
                Some(reason) => report
                    .excluded_files
                    .push(ExcludedFile { path: norm, reason }),
                None => report.reviewable_files.push(norm),
            }
        }

        report
    }

    /// Resolves built-in language rules and repository path-specific rules for `files`, grouping
    /// files that share identical rule content to eliminate prompt duplication.
    #[must_use]
    pub fn resolve_rule_groups(&self, files: &[String]) -> Vec<ResolvedRuleGroup> {
        let mut grouped: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();

        for rel in files {
            let norm = rel.replace('\\', "/");
            let mut matched_custom: Vec<&RepoPathRule> = Vec::new();
            for rule in &self.config.rules {
                if rule_path_matches(&rule.path, &norm) {
                    matched_custom.push(rule);
                }
            }

            let include_builtin =
                matched_custom.is_empty() || matched_custom.iter().any(|r| r.merge_system_rule);

            if include_builtin {
                let (lang_id, lang_rule) = builtin_language_rule_for_path(&norm);
                grouped
                    .entry((
                        lang_id.to_string(),
                        "builtin".to_string(),
                        lang_rule.to_string(),
                    ))
                    .or_default()
                    .push(norm.clone());
            }

            for (idx, custom) in matched_custom.iter().enumerate() {
                let trimmed = custom.rule.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let src = self
                    .config_source
                    .clone()
                    .unwrap_or_else(|| "repo-rule".to_string());
                grouped
                    .entry((
                        format!("repo:{}#{}", custom.path, idx + 1),
                        src,
                        trimmed.to_string(),
                    ))
                    .or_default()
                    .push(norm.clone());
            }
        }

        grouped
            .into_iter()
            .map(|((rule_id, source, rule_text), mut files)| {
                files.sort();
                files.dedup();
                ResolvedRuleGroup {
                    rule_id,
                    source,
                    files,
                    rule_text,
                }
            })
            .collect()
    }

    /// Renders deduplicated rule groups into a concise prompt block for Reviewer subagents.
    #[must_use]
    pub fn format_rules_for_prompt(&self, files: &[String], max_chars: usize) -> String {
        let groups = self.resolve_rule_groups(files);
        if groups.is_empty() {
            return String::new();
        }
        let mut out = String::from("### Active Precision Review Rules\n");
        for g in &groups {
            let file_preview = if g.files.len() <= 4 {
                g.files.join(", ")
            } else {
                format!("{}, +{} more", g.files[..3].join(", "), g.files.len() - 3)
            };
            let block = format!(
                "\n#### Rule `{}` ({}) — applies to: `{}`\n{}\n",
                g.rule_id, g.source, file_preview, g.rule_text
            );
            if out.len() + block.len() > max_chars {
                out.push_str("\n_(Additional language rules truncated to fit context budget.)_\n");
                break;
            }
            out.push_str(&block);
        }
        out
    }
}

fn normalize_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .map(|p| p.trim().replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .collect()
}

fn rule_path_matches(pattern: &str, rel_path: &str) -> bool {
    let pat = pattern.trim().replace('\\', "/");
    let path = rel_path.trim().replace('\\', "/");
    if pat.is_empty() || path.is_empty() {
        return false;
    }
    if pat == path {
        return true;
    }
    // Support directory-prefix shorthand like `src/agent/`
    if pat.ends_with('/') && path.starts_with(&pat) {
        return true;
    }
    // Single-segment basename pattern (e.g. `*.rs` or `Cargo.toml`) matches at any depth
    if !pat.contains('/') && pat != "**" {
        let file_name = path.rsplit('/').next().unwrap_or(path.as_str());
        if wildcard_match_segment(&pat, file_name) {
            return true;
        }
    }
    let pat_segs: Vec<&str> = pat.split('/').filter(|s| !s.is_empty()).collect();
    let path_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    glob_match_segments(&pat_segs, &path_segs)
}

/// Polynomial-time segment glob matcher supporting `**` (0+ directories), `*`, and `?`.
fn glob_match_segments(pat_segs: &[&str], path_segs: &[&str]) -> bool {
    let m = pat_segs.len();
    let n = path_segs.len();
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;

    for pi in 1..=m {
        if pat_segs[pi - 1] == "**" {
            dp[pi][0] = dp[pi - 1][0];
            for si in 1..=n {
                dp[pi][si] = dp[pi - 1][si] || dp[pi][si - 1];
            }
        } else {
            for si in 1..=n {
                if dp[pi - 1][si - 1] && wildcard_match_segment(pat_segs[pi - 1], path_segs[si - 1])
                {
                    dp[pi][si] = true;
                }
            }
        }
    }

    dp[m][n]
}

/// Linear-time wildcard matcher for a single path segment (`*` and `?`).
fn wildcard_match_segment(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(si) = star_idx {
            pi = si + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

/// Maps a file path to its built-in precision language/config rule checklist.
/// Rules emphasize precision-over-recall and avoid duplicating deterministic compiler/linter checks.
#[must_use]
pub fn builtin_language_rule_for_path(rel_path: &str) -> (&'static str, &'static str) {
    let lower = rel_path.replace('\\', "/").to_ascii_lowercase();
    let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());

    if file_name == "cargo.toml" {
        return ("cargo_toml", RULE_CARGO_TOML);
    }
    if file_name == "package.json" {
        return ("package_json", RULE_PACKAGE_JSON);
    }
    if lower.starts_with(".github/workflows/")
        && (lower.ends_with(".yml") || lower.ends_with(".yaml"))
    {
        return ("github_workflows", RULE_GITHUB_WORKFLOWS);
    }
    if file_name == "dockerfile"
        || file_name.starts_with("dockerfile.")
        || file_name.starts_with("docker-compose")
    {
        return ("docker", RULE_DOCKER);
    }

    if lower.ends_with(".rs") {
        return ("rust", RULE_RUST);
    }
    if lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
    {
        return ("ts_js_tsx_jsx", RULE_TS_JS);
    }
    if lower.ends_with(".py") {
        return ("python", RULE_PYTHON);
    }
    if lower.ends_with(".go") {
        return ("go", RULE_GO);
    }
    if lower.ends_with(".sql") || file_name == "schema.prisma" {
        return ("sql", RULE_SQL);
    }
    if lower.ends_with(".java") || lower.ends_with(".kt") || lower.ends_with(".kts") {
        return ("java_kotlin", RULE_JAVA_KOTLIN);
    }
    if lower.ends_with(".c")
        || lower.ends_with(".cc")
        || lower.ends_with(".cpp")
        || lower.ends_with(".h")
        || lower.ends_with(".hpp")
    {
        return ("c_cpp", RULE_C_CPP);
    }

    ("default", RULE_DEFAULT)
}

const RULE_DEFAULT: &str = "\
- Favor precision over recall: report only concrete defects likely real in changed code and reachable context. Never invent speculative issues.\n\
- Do not duplicate deterministic compiler/formatter diagnostics unless the diff introduces a concrete runtime or security bug.\n\
- Check boundary/null inputs, error propagation, resource leaks, broken cross-file contracts, and injection/auth flaws.\n\
- Classify correctness, soundness, and security as blocking (`critical`/`high`/`medium`); style-only nits are non-blocking (`low`).";

const RULE_RUST: &str = "\
- Precision first: do not repeat what `rustc`, borrowck, `clippy`, or `rustfmt` already enforce.\n\
- Panics & Errors: flag `.unwrap()`, `.expect()`, `panic!`, `todo!()`, `unimplemented!()`, or unchecked slice indexing on untrusted/runtime data in non-test production code.\n\
- Async & Concurrency: flag synchronous `Mutex`/`RwLock`/`RefCell` guards held across `.await`, blocking I/O inside async tasks, unbounded channels under untrusted load, and check-then-act races.\n\
- Unsafe & FFI: verify documented safety invariants, null/bounds/alignment checks, and `Send`/`Sync` soundness on every touched `unsafe` block.\n\
- Lifetimes & Allocation: flag accidental `Rc`/`Arc` reference cycles (`Weak` needed) or hot-loop `clone()`/`format!()` regressions.";

const RULE_TS_JS: &str = "\
- Precision first: do not repeat TypeScript typechecker or ESLint style diagnostics.\n\
- Runtime Safety & Parsing: check unvalidated `JSON.parse` / `localStorage` / API payloads, `NaN`/`Infinity` math coercion, unsafe object/map lookups on untrusted keys (prototype pollution or `undefined` property crash), and unhandled Promise rejections.\n\
- React Lifecycle & State: flag side effects inside state updaters (StrictMode double-invoke bugs), stale closures or missing dependencies in `useEffect`/`useMemo`, memory/timer/event-listener leaks on unmount, and synchronous `setState` inside `useEffect`.\n\
- Security: check `dangerouslySetInnerHTML`, unsanitized URLs (`javascript:`), open redirects, and leaked secrets in client bundles.\n\
- Cross-File Contracts: verify shared types, props, routes, and storage schemas remain consistent across callers and tests.";

const RULE_PYTHON: &str = "\
- Precision first: verify call sites before making non-local claims.\n\
- Correctness: flag mutable default arguments, late-binding loop closures, silent broad `except:` swallowing errors, naive vs timezone-aware `datetime` mixing, and unclosed file/network handles.\n\
- Async: flag blocking I/O (`requests`, `time.sleep`, sync DB calls) inside `async def` handlers.\n\
- Security: flag SQL string interpolation, `shell=True` subprocess calls with untrusted input, `yaml.load` without `SafeLoader`, `pickle.loads` on untrusted bytes, and path traversal on joined paths.";

const RULE_GO: &str = "\
- Precision first: do not duplicate `go vet` or `gofmt`.\n\
- Concurrency & Leaks: flag goroutines without cancellation/exit paths, missing `defer cancel()` / `resp.Body.Close()`, data races on shared maps/slices, and copying mutexes by value.\n\
- Correctness: flag ignored `err` returns, nil pointer/interface dereferences, loop-variable capture issues, and unchecked type assertions.\n\
- Security: flag SQL concatenation, command injection, SSRF, and path traversal via uncleaned `filepath.Join`.";

const RULE_SQL: &str = "\
- Security & Integrity: flag dynamic SQL concatenation (SQL injection), missing transactions around multi-step mutations, destructive `UPDATE`/`DELETE` without `WHERE`, and unindexed foreign keys on high-write tables.\n\
- Migrations: check backward-incompatible column drops/renames, lock-heavy DDL on live tables, and missing `NOT NULL` defaults.";

const RULE_JAVA_KOTLIN: &str = "\
- Correctness: flag unchecked null dereferences (NPE), unclosed `AutoCloseable` resources, broken `equals`/`hashCode` contracts, and integer/BigDecimal precision bugs in financial math.\n\
- Concurrency: flag unsynchronized shared mutable state, double-checked locking without `volatile`, and blocking calls on reactive/coroutine dispatchers.\n\
- Security: flag SQLi, XXE, insecure deserialization, SSRF, and missing authorization checks on controller endpoints.";

const RULE_C_CPP: &str = "\
- Memory Safety: flag buffer overflows, out-of-bounds pointer arithmetic, use-after-free, double-free, uninitialized reads, missing null checks after allocation, and integer overflows in size calculations.\n\
- Concurrency & RAII: flag data races, lock-order deadlocks, and leaked descriptors/handles on early error returns.";

const RULE_CARGO_TOML: &str = "\
- Dependencies: flag mismatched Tree-sitter/ABI crate versions, accidental heavyweight native C/OpenSSL dependencies where pure-Rust TLS is expected, wildcard `*` versions, and unintended feature bloat.";

const RULE_PACKAGE_JSON: &str = "\
- Scripts & Supply Chain: flag broken/untested scripts, unused or mismatched peer dependencies, `postinstall` script risks, and missing build/typecheck/test scripts expected by CI.";

const RULE_GITHUB_WORKFLOWS: &str = "\
- CI Security: flag `${{ github.event.* }}` untrusted interpolation inside `run:` shell scripts (script injection), overly broad `permissions: write-all`, `pull_request_target` checkout of untrusted PR code, and unpinned third-party actions.";

const RULE_DOCKER: &str = "\
- Container Security & Size: flag running as `root` in final stage, secrets baked into `ENV`/`ARG`/`COPY` layers, unpinned `:latest` base images, and missing `.dockerignore` / multi-stage build hygiene.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_path_and_noise_exclusions() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = ReviewRuleEngine::load(tmp.path(), None);

        let candidates = vec![
            "src/main.rs".to_string(),
            ".env".to_string(),
            ".env.production".to_string(),
            ".env.example".to_string(),
            "certs/server.pem".to_string(),
            "Cargo.lock".to_string(),
            "dist/bundle.min.js".to_string(),
            "assets/logo.png".to_string(),
            "src/lib/storage.ts".to_string(),
        ];

        let sel = engine.filter_files(&candidates);
        assert_eq!(
            sel.reviewable_files,
            vec![
                "src/main.rs".to_string(),
                ".env.example".to_string(),
                "src/lib/storage.ts".to_string()
            ]
        );
        assert!(sel
            .excluded_files
            .iter()
            .any(|e| e.path == ".env" && e.reason == ExclusionReason::SecretPath));
        assert!(sel
            .excluded_files
            .iter()
            .any(|e| e.path == "certs/server.pem" && e.reason == ExclusionReason::SecretPath));
        assert!(sel
            .excluded_files
            .iter()
            .any(|e| e.path == "Cargo.lock" && e.reason == ExclusionReason::Lockfile));
        assert!(sel
            .excluded_files
            .iter()
            .any(|e| e.path == "assets/logo.png" && e.reason == ExclusionReason::BinaryOrMedia));
    }

    #[test]
    fn test_loads_opencodereview_rule_json_and_merges_or_replaces_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let ocr_dir = tmp.path().join(".opencodereview");
        std::fs::create_dir_all(&ocr_dir).unwrap();
        std::fs::write(
            ocr_dir.join("rule.json"),
            r#"{
  "rules": [
    {
      "path": "src/auth/*.rs",
      "rule": "Verify constant-time token comparisons on all auth paths.",
      "merge_system_rule": true
    },
    {
      "path": "src/generated.rs",
      "rule": "Only verify schema version header.",
      "merge_system_rule": false
    }
  ],
  "exclude": ["src/fixtures/**"]
}"#,
        )
        .unwrap();

        let engine = ReviewRuleEngine::load(tmp.path(), None);
        assert_eq!(engine.config_source(), Some(OPEN_CODE_REVIEW_RULES_PATH));

        let sel = engine.filter_files(&[
            "src/auth/jwt.rs".to_string(),
            "src/generated.rs".to_string(),
            "src/fixtures/sample.json".to_string(),
        ]);
        assert_eq!(
            sel.reviewable_files,
            vec![
                "src/auth/jwt.rs".to_string(),
                "src/generated.rs".to_string()
            ]
        );
        assert_eq!(sel.excluded_files.len(), 1);
        assert_eq!(
            sel.excluded_files[0].reason,
            ExclusionReason::CustomExcludeRule
        );

        let groups = engine.resolve_rule_groups(&sel.reviewable_files);
        // `src/auth/jwt.rs` merged built-in `rust` + custom rule; `src/generated.rs` replaced built-in `rust`.
        let rust_group = groups.iter().find(|g| g.rule_id == "rust").unwrap();
        assert_eq!(rust_group.files, vec!["src/auth/jwt.rs".to_string()]);
        assert!(groups
            .iter()
            .any(|g| g.rule_text.contains("constant-time token comparisons")));
        assert!(groups.iter().any(
            |g| g.rule_text.contains("Only verify schema version header")
                && g.files == vec!["src/generated.rs".to_string()]
        ));
    }

    #[test]
    fn test_glob_matching_double_star_single_star_and_question_mark() {
        assert!(rule_path_matches("src/**/*.rs", "src/main.rs"));
        assert!(rule_path_matches("src/**/*.rs", "src/a/b/c.rs"));
        assert!(!rule_path_matches("src/auth/*.rs", "src/auth/sub/jwt.rs"));
        assert!(rule_path_matches("src/auth/*.rs", "src/auth/jwt.rs"));
        assert!(rule_path_matches("src/v?/api.ts", "src/v2/api.ts"));
        assert!(!rule_path_matches("src/v?/api.ts", "src/v12/api.ts"));
        assert!(rule_path_matches("src/agent/", "src/agent/loop.rs"));
        assert!(rule_path_matches("*.toml", "nested/Cargo.toml"));
    }
}
