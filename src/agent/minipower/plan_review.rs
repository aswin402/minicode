//! Plan review for `create_plan` / `power_plan`.
//!
//! Two layers, cheapest first:
//! 1. **Structural checks** (deterministic, no LLM): facts the code can prove, e.g. a target path
//!    escapes the workspace, a test command's program is not installed, or open design questions
//!    were never answered. Structural *blocking* findings keep the discovery write-guard active.
//! 2. **Semantic LLM review** (optional): runs only when structural checks found no blockers and
//!    the configured mode asks for it. Its findings are always *advisory* — a model opinion can
//!    never block execution.

use std::path::Path;
use std::sync::Arc;

use crate::agent::minipower::{CeremonySize, MiniPowerEngine, PlanningPhaseState};

/// Inputs describing a proposed plan.
#[derive(Debug, Clone, Copy)]
pub struct PlanReviewInput<'a> {
    pub topic: &'a str,
    pub steps: &'a [String],
    pub target_files: &'a [String],
    pub test_command: Option<&'a str>,
}

/// Review outcome. `blocking` findings are proven structural failures; `advisory` findings are
/// warnings (structural or LLM).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanReviewReport {
    pub blocking: Vec<String>,
    pub advisory: Vec<String>,
    /// Status line about the semantic review (ran / skipped / unavailable).
    pub llm_note: Option<String>,
}

impl PlanReviewReport {
    pub fn is_blocked(&self) -> bool {
        !self.blocking.is_empty()
    }

    /// Markdown summary appended to plan tool output.
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        if self.blocking.is_empty() && self.advisory.is_empty() {
            out.push_str("\n• Plan review: structural checks passed.");
        }
        if !self.blocking.is_empty() {
            out.push_str("\n• Plan review — blocking (structural):");
            for b in &self.blocking {
                out.push_str(&format!("\n  ✖ {}", b));
            }
        }
        if !self.advisory.is_empty() {
            out.push_str("\n• Plan review — advisory:");
            for a in &self.advisory {
                out.push_str(&format!("\n  ! {}", a));
            }
        }
        if let Some(note) = &self.llm_note {
            out.push_str(&format!("\n• {}", note));
        }
        out
    }
}

/// When to run the semantic LLM review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanReviewMode {
    /// Structural checks only.
    Structural,
    /// LLM review only for project-sized work (default).
    Auto,
    /// LLM review on every plan.
    Llm,
}

impl PlanReviewMode {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "llm" | "always" | "on" => Self::Llm,
            "structural" | "off" | "none" | "rules" => Self::Structural,
            _ => Self::Auto,
        }
    }

    /// Resolves the effective mode from config (`agent.plan_review`, env `MINICODE_PLAN_REVIEW`).
    /// Library unit tests never call a provider unless the env var is set explicitly.
    pub fn resolve(workspace_root: &Path) -> Self {
        let explicit_env = std::env::var(crate::constants::env_vars::MINICODE_PLAN_REVIEW).ok();
        if cfg!(test) && explicit_env.is_none() {
            return Self::Structural;
        }
        let cfg = crate::config::Config::load_for_workspace(workspace_root);
        Self::parse(&cfg.agent.plan_review)
    }
}

/// POSIX shell builtins that are valid as the first word of a test command but are not
/// executables on `PATH`.
const SHELL_BUILTINS: &[&str] = &[
    "cd", "export", "source", ".", "set", "unset", "true", "false", ":", "test", "[", "env",
    "exec", "eval",
];

/// Returns programs referenced by a shell command that cannot be resolved (not a builtin, not on
/// `PATH`, and not an existing relative/absolute path).
fn unresolved_programs(workspace_root: &Path, command: &str) -> Vec<String> {
    let mut missing = Vec::new();
    for segment in command.split(['&', '|', ';']) {
        let program = segment
            .split_whitespace()
            .find(|w| !(w.contains('=') && !w.starts_with('=') && !w.contains('/')));
        let Some(program) = program else {
            continue;
        };
        if SHELL_BUILTINS.contains(&program) {
            continue;
        }
        let resolvable = if program.contains('/') {
            let p = Path::new(program);
            if p.is_absolute() {
                p.exists()
            } else {
                workspace_root.join(p).exists()
            }
        } else {
            which::which(program).is_ok()
        };
        if !resolvable && !missing.iter().any(|m| m == program) {
            missing.push(program.to_string());
        }
    }
    missing
}

/// Normalizes free text for loose containment checks (lowercase, collapsed whitespace).
fn normalize_text(s: &str) -> String {
    s.to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns true if a whitespace-delimited token inside a step description syntactically looks like
/// a relative/absolute file path with a valid alphanumeric file extension (excluding URLs and ratios like `1440/768`).
fn looks_like_workspace_file_token(token: &str) -> bool {
    if !token.contains('/')
        || token.starts_with("http://")
        || token.starts_with("https://")
        || token.starts_with("//")
    {
        return false;
    }
    Path::new(token)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            (1..=10).contains(&ext.len())
                && ext.starts_with(|c: char| c.is_ascii_alphabetic())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

/// Deterministic structural review (no LLM, no network).
pub fn review_structural(workspace_root: &Path, input: PlanReviewInput<'_>) -> PlanReviewReport {
    let mut report = PlanReviewReport::default();

    if input.steps.is_empty() {
        report
            .blocking
            .push("Plan has no steps — add concrete, verifiable steps.".to_string());
    }

    // Target files: escaping the workspace is impossible to execute; new directories are fine.
    for target in input.target_files {
        match MiniPowerEngine::workspace_relative(workspace_root, target) {
            None => report
                .blocking
                .push(format!("Target `{}` is outside the workspace.", target)),
            Some(rel) => {
                let parent_exists = Path::new(&rel)
                    .parent()
                    .map(|p| p.as_os_str().is_empty() || workspace_root.join(p).is_dir())
                    .unwrap_or(true);
                if !parent_exists {
                    report.advisory.push(format!(
                        "Target `{}` lives in a directory that does not exist yet (it will be created).",
                        rel
                    ));
                }
            }
        }
    }

    // Paths mentioned inline inside step descriptions: flag workspace-escaping paths as blocking
    // and non-existent parent directories as advisory.
    let mut missing_parent_paths: Vec<String> = Vec::new();
    for step in input.steps {
        for raw_token in step.split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '`' | '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';' | ':'
                )
        }) {
            let token = raw_token.trim_matches('.');
            if !looks_like_workspace_file_token(token) {
                continue;
            }
            match MiniPowerEngine::workspace_relative(workspace_root, token) {
                None => report.blocking.push(format!(
                    "Step references path `{}` outside the workspace.",
                    token
                )),
                Some(rel) => {
                    if !workspace_root.join(&rel).exists() {
                        let parent_exists = Path::new(&rel)
                            .parent()
                            .map(|p| p.as_os_str().is_empty() || workspace_root.join(p).is_dir())
                            .unwrap_or(true);
                        if !parent_exists && !missing_parent_paths.contains(&rel) {
                            missing_parent_paths.push(rel);
                        }
                    }
                }
            }
        }
    }
    if !missing_parent_paths.is_empty() {
        report.advisory.push(format!(
            "Step(s) reference path(s) in directories that do not exist yet: `{}`.",
            missing_parent_paths.join("`, `")
        ));
    }

    // Test command must be runnable; a missing one is only a warning.
    match input.test_command.map(str::trim).filter(|c| !c.is_empty()) {
        Some(cmd) => {
            let missing = unresolved_programs(workspace_root, cmd);
            if !missing.is_empty() {
                report.blocking.push(format!(
                    "Test command `{}` uses program(s) not found on PATH: {}.",
                    cmd,
                    missing.join(", ")
                ));
            }
        }
        None if !input.target_files.is_empty() => report.advisory.push(
            "No `test_command` given for a plan that modifies files; verification will rely on `power_verify`."
                .to_string(),
        ),
        None => {}
    }

    // Open questions from the discovery phase must be answered before execution.
    let decisions = crate::agent::decisions::read_for_prompt(workspace_root);
    if let PlanningPhaseState::DiscoveryWriteGuard { open_questions, .. } =
        MiniPowerEngine::get_planning_phase_state(workspace_root)
    {
        if !open_questions.is_empty() {
            match decisions.as_deref() {
                None => report.blocking.push(format!(
                    "{} open design question(s) from discovery but no decisions recorded in `.minicode/decisions.md`: {}.",
                    open_questions.len(),
                    open_questions.join("; ")
                )),
                Some(body) => {
                    let body_norm = normalize_text(body);
                    let unanswered: Vec<&String> = open_questions
                        .iter()
                        .filter(|q| !body_norm.contains(&normalize_text(q)))
                        .collect();
                    if !unanswered.is_empty() {
                        report.advisory.push(format!(
                            "Open question(s) not found verbatim in `.minicode/decisions.md` (confirm they are settled): {}.",
                            unanswered
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join("; ")
                        ));
                    }
                }
            }
        }
    }

    let terse = input
        .steps
        .iter()
        .filter(|s| s.split_whitespace().count() < 2)
        .count();
    if terse > 0 {
        report.advisory.push(format!(
            "{} step(s) have fewer than 2 words — name the target module or behavior.",
            terse
        ));
    }

    if decisions.is_none() && input.steps.len() >= 4 {
        report.advisory.push(
            "Multi-step plan with no recorded decisions — log assumed stack/UX choices via `record_decision`."
                .to_string(),
        );
    }

    let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);
    if docs_dir.join("core/spec.md").is_file() || docs_dir.join("core/architecture.md").is_file() {
        let trace = super::speckit::analyze_workspace_artifacts(workspace_root);
        if let Some(hit) = trace.ambiguity_hits.first() {
            report.advisory.push(format!(
                "Spec has {} unresolved ambiguity marker(s) (e.g. `{}` at `{}:{}`).",
                trace.ambiguity_hits.len(),
                hit.marker,
                hit.source_file,
                hit.line
            ));
        }
    }

    report
}

/// Maximum characters of spec text included in the LLM review prompt.
const LLM_REVIEW_SPEC_CHARS: usize = 6000;
/// Upper bound on the LLM review call.
const LLM_REVIEW_TIMEOUT_SECS: u64 = 60;

fn build_llm_prompt(
    workspace_root: &Path,
    input: PlanReviewInput<'_>,
    structural: &PlanReviewReport,
) -> String {
    let spec = std::fs::read_to_string(crate::tools::minikit::resolve_doc_path(
        workspace_root,
        "spec.md",
    ))
    .unwrap_or_default();
    let spec: String = spec.chars().take(LLM_REVIEW_SPEC_CHARS).collect();
    let decisions = crate::agent::decisions::read_for_prompt(workspace_root).unwrap_or_default();
    let steps = input
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You are a cold-reader plan reviewer. You cannot talk to the author.\n\
         Topic: {topic}\n\n## Spec (may be empty)\n{spec}\n\n## Recorded decisions (may be empty)\n{decisions}\n\n\
         ## Plan steps\n{steps}\n\n## Target files\n{targets}\n\n## Test command\n{test}\n\n\
         ## Structural checks already performed (do not repeat them)\n{structural}\n\n\
         Report only semantic problems a builder would hit: steps that do not deliver the spec, missing edge cases, \
         wrong ordering, or untestable steps. Skip wording nitpicks. Respond with ONLY a JSON object: \
         {{\"advisory\": [\"short finding\", ...]}} (empty array if nothing important).",
        topic = input.topic,
        spec = spec,
        decisions = decisions,
        steps = steps,
        targets = input.target_files.join(", "),
        test = input.test_command.unwrap_or("(none)"),
        structural = if structural.advisory.is_empty() {
            "all passed".to_string()
        } else {
            structural.advisory.join("; ")
        }
    )
}

/// Extracts `advisory` findings from a model response (tolerates surrounding prose / fences).
fn parse_llm_findings(raw: &str) -> Option<Vec<String>> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&raw[start..=end]).ok()?;
    let items = value.get("advisory")?.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

/// Runs the semantic review with the given provider and model; findings become advisory.
pub async fn review_semantic(
    workspace_root: &Path,
    input: PlanReviewInput<'_>,
    report: &mut PlanReviewReport,
    provider: Arc<dyn crate::agent::provider::Provider>,
    model: &str,
) {
    let prompt = build_llm_prompt(workspace_root, input, report);
    let messages = vec![
        crate::agent::types::Message::system(
            "You review implementation plans and answer with strict JSON.".to_string(),
        ),
        crate::agent::types::Message::user(prompt),
    ];
    let options = crate::agent::provider::CompletionOptions::new(model);
    let call = provider.completion(&messages, &[], &options);
    match tokio::time::timeout(
        std::time::Duration::from_secs(LLM_REVIEW_TIMEOUT_SECS),
        call,
    )
    .await
    {
        Ok(Ok(text)) => match parse_llm_findings(&text) {
            Some(findings) => {
                report.llm_note = Some(format!(
                    "LLM plan review (advisory): {} finding(s).",
                    findings.len()
                ));
                report
                    .advisory
                    .extend(findings.into_iter().map(|f| format!("[LLM] {}", f)));
            }
            None => {
                report.llm_note =
                    Some("LLM plan review returned an unparseable response; skipped.".to_string())
            }
        },
        Ok(Err(e)) => report.llm_note = Some(format!("LLM plan review unavailable: {}", e)),
        Err(_) => {
            report.llm_note = Some(format!(
                "LLM plan review timed out after {}s; skipped.",
                LLM_REVIEW_TIMEOUT_SECS
            ))
        }
    }
}

/// Full review: structural first; the LLM review runs only when nothing blocks and the mode
/// requires it (`Auto` → project-sized work only).
pub async fn run_review(workspace_root: &Path, input: PlanReviewInput<'_>) -> PlanReviewReport {
    let mut report = review_structural(workspace_root, input);
    if report.is_blocked() {
        return report;
    }
    let mode = PlanReviewMode::resolve(workspace_root);
    let wants_llm = match mode {
        PlanReviewMode::Structural => false,
        PlanReviewMode::Llm => true,
        PlanReviewMode::Auto => {
            CeremonySize::classify(workspace_root, input.topic, None, 0) == CeremonySize::Project
        }
    };
    if !wants_llm {
        return report;
    }
    let provider = crate::agent::subagent::get_global_subagent_pool(workspace_root)
        .get_or_create_provider()
        .await;
    let model = crate::config::Config::load_for_workspace(workspace_root)
        .provider
        .model;
    review_semantic(workspace_root, input, &mut report, provider, &model).await;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        steps: &'a [String],
        targets: &'a [String],
        test: Option<&'a str>,
    ) -> PlanReviewInput<'a> {
        PlanReviewInput {
            topic: "Feature",
            steps,
            target_files: targets,
            test_command: test,
        }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn test_clean_plan_passes() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        let steps = s(&["Add parser module", "Write parser tests"]);
        let targets = s(&["src/parser.rs"]);
        let r = review_structural(dir.path(), input(&steps, &targets, Some("git status")));
        assert!(!r.is_blocked(), "{:?}", r);
        assert!(r.advisory.is_empty(), "{:?}", r);
    }

    #[test]
    fn test_blocking_findings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let empty: Vec<String> = vec![];
        assert!(review_structural(dir.path(), input(&empty, &empty, None)).is_blocked());

        let steps = s(&["Do the thing now"]);
        let outside = s(&["../elsewhere/x.rs"]);
        assert!(review_structural(dir.path(), input(&steps, &outside, None)).is_blocked());

        let r = review_structural(
            dir.path(),
            input(
                &steps,
                &empty,
                Some("cd sub && definitely-not-a-real-binary-xyz --run"),
            ),
        );
        assert!(r.is_blocked());
        assert!(r.blocking[0].contains("definitely-not-a-real-binary-xyz"));
        assert!(!r.blocking[0].contains("`cd`,"));
    }

    #[test]
    fn test_open_questions_without_decisions_block() {
        let dir = tempfile::tempdir().expect("tempdir");
        MiniPowerEngine::set_planning_phase_state(
            dir.path(),
            &PlanningPhaseState::DiscoveryWriteGuard {
                topic: "T".to_string(),
                open_questions: vec!["Session vs JWT?".to_string()],
                enforce_strict: true,
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .expect("guard");
        let steps = s(&["Implement auth middleware"]);
        let empty: Vec<String> = vec![];
        let r = review_structural(dir.path(), input(&steps, &empty, None));
        assert!(r.is_blocked());
        assert!(r.blocking[0].contains("Session vs JWT?"));
    }

    #[test]
    fn test_advisories_do_not_block() {
        let dir = tempfile::tempdir().expect("tempdir");
        let steps = s(&["Refactor", "Add new handlers module"]);
        let targets = s(&["src/new_mod/handlers.rs"]);
        let r = review_structural(dir.path(), input(&steps, &targets, None));
        assert!(!r.is_blocked());
        assert_eq!(r.advisory.len(), 3, "{:?}", r.advisory);
    }

    #[test]
    fn test_parse_llm_findings() {
        let raw =
            "Here you go:\n```json\n{\"advisory\": [\"Missing empty-input case\", \" \"]}\n```";
        assert_eq!(
            parse_llm_findings(raw),
            Some(vec!["Missing empty-input case".to_string()])
        );
        assert_eq!(parse_llm_findings("no json here"), None);
    }

    #[test]
    fn test_mode_parse() {
        assert_eq!(PlanReviewMode::parse("LLM"), PlanReviewMode::Llm);
        assert_eq!(
            PlanReviewMode::parse("structural"),
            PlanReviewMode::Structural
        );
        assert_eq!(PlanReviewMode::parse("auto"), PlanReviewMode::Auto);
        assert_eq!(PlanReviewMode::parse("garbage"), PlanReviewMode::Auto);
    }

    #[tokio::test]
    async fn test_semantic_review_with_mock_provider_is_advisory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let steps = s(&["Add parser module"]);
        let empty: Vec<String> = vec![];
        let mut report = review_structural(dir.path(), input(&steps, &empty, None));
        let mock = crate::agent::mock_provider::MockProvider::new("mock", "mock");
        mock.push_response(
            &["{\"advisory\": [\"Step does not cover error handling\"]}"],
            vec![],
        );
        let provider: Arc<dyn crate::agent::provider::Provider> = Arc::new(mock);
        review_semantic(
            dir.path(),
            input(&steps, &empty, None),
            &mut report,
            provider,
            "mock",
        )
        .await;
        assert!(!report.is_blocked());
        assert!(report
            .advisory
            .iter()
            .any(|a| a.contains("[LLM] Step does not cover error handling")));
    }
}
