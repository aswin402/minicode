//! Tool Middleware Pipeline
//!
//! Provides a composable `ToolMiddleware` trait and a `ToolPipeline` runner
//! that wraps every tool execution — both built-in and MCP — with ordered
//! before/after hooks.
//!
//! Built-in middlewares (applied in order):
//! 1. [`TimingMiddleware`]     — records execution span via `tracing`
//! 2. [`CheckpointMiddleware`] — logs destructive tool calls for telemetry
//! 3. [`DiffMiddleware`]       — prepends inline diff for file-modifying tools
//! 4. [`RedactMiddleware`]     — strips secrets from tool output and diff display
//! 5. [`SpilloverMiddleware`]  — persists full redacted output to `.minicode/tmp/tool_outputs/` before truncation

use crate::agent::types::ToolResult;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const FILE_PATH_ALIASES: &[&str] = &["path", "file_path", "filepath", "file", "target_file"];
const MUTATION_SCOPE_ALIASES: &[&str] = &["glob", "pattern", "directory", "target_dir", "dir"];
const MAX_SPILLOVER_FILES: usize = 20;
static SPILLOVER_SEQ: AtomicU64 = AtomicU64::new(1);

/// Persists large or compacted tool output to `.minicode/tmp/tool_outputs/<ts>_<seq>_<tool>.log`
/// after secret redaction, maintaining a bounded ring of at most `MAX_SPILLOVER_FILES` logs.
/// Returns the workspace-relative path if successfully written.
pub fn save_spillover_output(
    workspace_root: &Path,
    tool_name: &str,
    raw_output: &str,
) -> Option<String> {
    let dir = workspace_root
        .join(crate::constants::WORKSPACE_DIR_NAME)
        .join("tmp")
        .join("tool_outputs");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }

    let safe_tool: String = tool_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = SPILLOVER_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = format!("{:013}_{:04}_{}.log", ts_ms, seq % 10_000, safe_tool);
    let full_path = dir.join(&file_name);

    let redacted = crate::sandbox::redact::SecretRedactor::global().redact(raw_output);
    if std::fs::write(&full_path, redacted.as_bytes()).is_err() {
        return None;
    }

    // Prune oldest spillover files beyond MAX_SPILLOVER_FILES
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut files: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok().map(|de| de.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("log"))
            .collect();
        if files.len() > MAX_SPILLOVER_FILES {
            files.sort();
            let remove_count = files.len() - MAX_SPILLOVER_FILES;
            for old in files.into_iter().take(remove_count) {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    Some(format!(
        "{}/tmp/tool_outputs/{}",
        crate::constants::WORKSPACE_DIR_NAME,
        file_name
    ))
}

// ── Context ───────────────────────────────────────────────────────────────────

/// Read-only context passed to each middleware.
pub struct ToolContext<'a> {
    /// Name of the tool that was called.
    pub tool_name: &'a str,
    /// Workspace root directory — available for middlewares that need path context.
    #[allow(dead_code)]
    pub workspace_root: &'a Path,
    /// Raw arguments passed to the tool (JSON).
    pub args: &'a serde_json::Value,
    /// Contents of the target file **before** tool execution.
    /// `Some(content)` for `write_file` / `patch_file` when the file existed
    /// prior to dispatch; `None` otherwise.
    pub file_before: Option<&'a str>,
}

// ── Admission Control ────────────────────────────────────────────────────────

/// Outcome of a pre-execution `ToolMiddleware::before` admission check.
pub enum AdmissionDecision {
    /// Proceed with normal tool execution.
    Allow,
    /// Short-circuit tool execution and return this result directly without invoking the tool.
    ShortCircuit(ToolResult),
}

/// Closed-form economic + context-pressure admission policy (`shunt` principle, zero magic line counts).
///
/// Computes a dynamic byte ceiling from the active context window $W$, current utilization $U$,
/// and marginal price ratio between the primary model and a verified `SemanticTier::Fast` worker
/// model so oversized unbounded reads are intercepted *before* flooding the main transcript.
pub struct EconomicAdmissionPolicy;

impl EconomicAdmissionPolicy {
    /// Expected amortized turn persistence factor before context compaction/caching discounts it.
    const AMORTIZED_TURN_FACTOR: f64 = 4.0;
    /// Expected concise summary/outline token footprint when shunted or clamped.
    const EXPECTED_SUMMARY_TOKENS: f64 = 300.0;
    /// Minimum admission token ceiling (prevents over-eager interception on tiny files).
    const MIN_CEILING_TOKENS: usize = 1_500;
    /// Maximum admission token ceiling even on huge 1M+ windows.
    const MAX_CEILING_TOKENS: usize = 6_000;

    /// Computes the dynamic admission ceiling in estimated tokens using pure closed-form math:
    /// 1. Context-pressure ceiling $\tau_{\text{ctx}}(W, U) = \text{clamp}(\lfloor 0.04 \cdot W \cdot (1 - U)^2 \rfloor, 1500, 6000)$
    /// 2. Cost-amortization break-even $N^*$ when a strictly cheaper `Fast` tier is available:
    ///    $N \cdot c_{\text{in}}^{\text{main}} \cdot \gamma > N \cdot c_{\text{in}}^{\text{fast}} + K_{\text{out}} (c_{\text{out}}^{\text{fast}} + c_{\text{in}}^{\text{main}} \cdot \gamma)$
    pub fn compute_admission_token_ceiling(
        context_window: usize,
        context_used: usize,
        main_in_rate: f64,
        fast_in_rate: Option<f64>,
        fast_out_rate: Option<f64>,
    ) -> usize {
        let w = if context_window > 0 {
            context_window
        } else {
            128_000
        };
        let u = (context_used as f64 / w as f64).clamp(0.0, 0.98);
        let ctx_ceiling = ((w as f64) * 0.04 * (1.0 - u).powi(2)).floor() as usize;
        let clamped_ctx = ctx_ceiling.clamp(Self::MIN_CEILING_TOKENS, Self::MAX_CEILING_TOKENS);

        if let (Some(f_in), Some(f_out)) = (fast_in_rate, fast_out_rate) {
            let main_amortized = main_in_rate * Self::AMORTIZED_TURN_FACTOR;
            if main_in_rate > 0.0 && f_in < main_amortized {
                let denom = main_amortized - f_in;
                if denom > f64::EPSILON {
                    let numer = Self::EXPECTED_SUMMARY_TOKENS * (f_out + main_amortized);
                    let breakeven = (numer / denom).ceil() as usize;
                    let bounded_be =
                        breakeven.clamp(Self::MIN_CEILING_TOKENS, Self::MAX_CEILING_TOKENS);
                    return clamped_ctx.min(bounded_be);
                }
            }
        }

        clamped_ctx
    }

    /// Resolves the live byte ceiling (`tokens * 4`) for the active workspace/session.
    pub fn current_byte_ceiling(workspace_root: &Path) -> usize {
        let ctx_window = crate::tools::exec::resolve_context_window(workspace_root);
        let ctx_used = crate::context::budget::donut::get_active_context_used();

        let (provider_name, main_model) = Self::resolve_active_provider_and_model(workspace_root);
        let (main_in, _) =
            crate::agent::pricing::ModelPricing::input_output_rates(&provider_name, &main_model);
        let (fast_in, fast_out) =
            crate::agent::models::DynamicModelResolver::resolve_verified_cheaper_fast_model(
                &provider_name,
                &main_model,
            )
            .map(|fm| {
                let (fi, fo) =
                    crate::agent::pricing::ModelPricing::input_output_rates(&provider_name, &fm);
                (Some(fi), Some(fo))
            })
            .unwrap_or((None, None));

        let token_ceiling =
            Self::compute_admission_token_ceiling(ctx_window, ctx_used, main_in, fast_in, fast_out);
        token_ceiling.saturating_mul(4)
    }

    fn resolve_active_provider_and_model(workspace_root: &Path) -> (String, String) {
        let prov = std::env::var("MINICODE_PROVIDER")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                crate::config::Config::load_for_workspace(workspace_root)
                    .provider
                    .default
            });
        let model = std::env::var("MINICODE_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| crate::agent::subagent::get_live_orchestrator_model_for_provider(&prov))
            .unwrap_or_else(|| {
                crate::config::Config::load_for_workspace(workspace_root)
                    .get_default_model_for_provider(&prov)
            });
        (prov, model)
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// A composable hook applied before and/or after tool execution.
///
/// Implementations may intercept calls in `before` for pre-execution admission control
/// and transform completed outputs in `after`. Middlewares run in declaration order.
pub trait ToolMiddleware: Send + Sync {
    /// Pre-execution admission check. Defaults to [`AdmissionDecision::Allow`].
    fn before(&self, _ctx: &ToolContext<'_>) -> AdmissionDecision {
        AdmissionDecision::Allow
    }

    /// Post-execution result transformation. Defaults to pass-through.
    fn after(&self, _ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        result
    }
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

/// Ordered pipeline of [`ToolMiddleware`] implementations.
pub struct ToolPipeline {
    middlewares: Vec<Box<dyn ToolMiddleware>>,
}

impl Default for ToolPipeline {
    /// Default pipeline: `ReadAdmission (before) → Timing → Checkpoint → Diff → Redact → Spillover`
    fn default() -> Self {
        Self {
            middlewares: vec![
                Box::new(ReadAdmissionMiddleware),
                Box::new(TimingMiddleware),
                Box::new(CheckpointMiddleware),
                Box::new(DiffMiddleware),
                Box::new(RedactMiddleware),
                Box::new(SpilloverMiddleware),
            ],
        }
    }
}

impl ToolPipeline {
    #[allow(dead_code)]
    pub fn new(middlewares: Vec<Box<dyn ToolMiddleware>>) -> Self {
        Self { middlewares }
    }

    /// Runs all middlewares' `before` admission hooks in order, short-circuiting on the first
    /// [`AdmissionDecision::ShortCircuit`].
    pub fn run_before(
        &self,
        tool_id: &str,
        tool_name: &str,
        workspace_root: &Path,
        args: &serde_json::Value,
    ) -> AdmissionDecision {
        let ctx = ToolContext {
            tool_name,
            workspace_root,
            args,
            file_before: None,
        };
        for mw in &self.middlewares {
            if let AdmissionDecision::ShortCircuit(mut res) = mw.before(&ctx) {
                res.tool_id = tool_id.to_string();
                res.tool_name = tool_name.to_string();
                return AdmissionDecision::ShortCircuit(res);
            }
        }
        AdmissionDecision::Allow
    }

    /// Run all middlewares' `after` hooks in order.
    ///
    /// `file_before`: contents of the target file read **before** dispatch,
    /// used by `DiffMiddleware` to compute a before/after diff.
    pub fn run(
        &self,
        result: ToolResult,
        tool_name: &str,
        workspace_root: &Path,
        args: &serde_json::Value,
        file_before: Option<&str>,
    ) -> ToolResult {
        let ctx = ToolContext {
            tool_name,
            workspace_root,
            args,
            file_before,
        };
        self.middlewares
            .iter()
            .fold(result, |acc, mw| mw.after(&ctx, acc))
    }
}

// ── Built-in middlewares ───────────────────────────────────────────────────────

/// Pre-tool admission gate preventing raw shell file-dump commands (`cat`, `nl`, `tac`,
/// unbounded `head`/`tail`, `bat`, `more`, `less`) from bypassing `read_file`'s clamping +
/// Tree-sitter AST outline when targeting workspace files larger than the dynamic economic ceiling.
///
/// Uses a pure state-machine shell tokenizer (zero regexes): piped/filtered commands (e.g.
/// `cat foo.rs | grep bar`, `cat foo.rs | wc -l`) and explicitly bounded reads (`head -n 80`)
/// pass through untouched. Oversized unfiltered dumps short-circuit immediately into clamped
/// line-numbered output + AST symbol outline in 0 extra LLM turns.
pub struct ReadAdmissionMiddleware;

impl ReadAdmissionMiddleware {
    const DUMP_UTILITIES: &'static [&'static str] =
        &["cat", "nl", "tac", "head", "tail", "bat", "more", "less"];
    const SHELL_WRAPPERS: &'static [&'static str] =
        &["env", "command", "nohup", "nice", "timeout", "stdbuf"];

    /// Tokenizes a single simple shell command respecting single/double quotes and backslash escapes.
    /// Returns `None` if the command contains shell composition operators (`|`, `&&`, `||`, `;`,
    /// `\n`, `>`, `<`, `$(`, `` ` ``) or unquoted glob wildcards (`*`, `?`, `[`) so targeted
    /// pipelines and multi-step commands always pass straight through.
    fn tokenize_simple_unfiltered_command(cmd: &str) -> Option<Vec<String>> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        let mut chars = cmd.chars().peekable();

        while let Some(ch) = chars.next() {
            if escaped {
                current.push(ch);
                escaped = false;
                continue;
            }
            if in_single {
                if ch == '\'' {
                    in_single = false;
                } else {
                    current.push(ch);
                }
                continue;
            }
            if in_double {
                match ch {
                    '"' => in_double = false,
                    '\\' => escaped = true,
                    '`' => return None,
                    '$' if chars.peek() == Some(&'(') => return None,
                    _ => current.push(ch),
                }
                continue;
            }

            match ch {
                '\'' => in_single = true,
                '"' => in_double = true,
                '\\' => escaped = true,
                '|' | ';' | '\n' | '\r' | '&' | '>' | '<' | '`' => return None,
                '$' if chars.peek() == Some(&'(') => return None,
                '*' | '?' | '[' => return None,
                c if c.is_ascii_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            }
        }

        if in_single || in_double || escaped {
            return None;
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        if tokens.is_empty() {
            None
        } else {
            Some(tokens)
        }
    }

    /// Parses an unfiltered file-dump command into `(binary, explicit_small_bound, file_args)`.
    fn inspect_dump_command(
        tokens: &[String],
        byte_ceiling: usize,
    ) -> Option<(String, bool, Vec<String>)> {
        let mut idx = 0usize;
        while idx < tokens.len() {
            let tok = tokens[idx].as_str();
            if tok.contains('=') && !tok.starts_with('-') {
                idx += 1;
                continue;
            }
            if tok == "nice" {
                idx += 1;
                if idx < tokens.len() && tokens[idx] == "-n" {
                    idx += 2;
                }
                continue;
            }
            if tok == "timeout" {
                idx += 1;
                while idx < tokens.len() && tokens[idx].starts_with('-') {
                    idx += 1;
                }
                if idx < tokens.len() {
                    idx += 1;
                }
                continue;
            }
            if Self::SHELL_WRAPPERS.contains(&tok) {
                idx += 1;
                continue;
            }
            break;
        }

        let bin_raw = tokens.get(idx)?;
        let bin = bin_raw
            .rsplit('/')
            .next()
            .unwrap_or(bin_raw)
            .to_ascii_lowercase();
        if !Self::DUMP_UTILITIES.contains(&bin.as_str()) {
            return None;
        }

        let mut is_bounded_small = false;
        let mut files = Vec::new();
        let mut i = idx + 1;
        let mut after_double_dash = false;
        let is_head_or_tail = bin == "head" || bin == "tail";

        while i < tokens.len() {
            let arg = tokens[i].as_str();
            if !after_double_dash && arg == "--" {
                after_double_dash = true;
                i += 1;
                continue;
            }
            if !after_double_dash && arg.starts_with('-') && arg != "-" {
                if is_head_or_tail {
                    if arg == "-n" || arg == "--lines" {
                        if let Some(val) = tokens.get(i + 1) {
                            if Self::is_small_line_bound(val) {
                                is_bounded_small = true;
                            }
                            i += 2;
                            continue;
                        }
                    } else if let Some(val) = arg
                        .strip_prefix("-n")
                        .or_else(|| arg.strip_prefix("--lines="))
                    {
                        if Self::is_small_line_bound(val) {
                            is_bounded_small = true;
                        }
                        i += 1;
                        continue;
                    } else if arg == "-c" || arg == "--bytes" {
                        if let Some(val) = tokens.get(i + 1) {
                            if Self::is_small_byte_bound(val, byte_ceiling) {
                                is_bounded_small = true;
                            }
                            i += 2;
                            continue;
                        }
                    } else if let Some(val) = arg
                        .strip_prefix("-c")
                        .or_else(|| arg.strip_prefix("--bytes="))
                    {
                        if Self::is_small_byte_bound(val, byte_ceiling) {
                            is_bounded_small = true;
                        }
                        i += 1;
                        continue;
                    } else if arg.len() > 1 && arg[1..].chars().all(|c| c.is_ascii_digit()) {
                        if Self::is_small_line_bound(&arg[1..]) {
                            is_bounded_small = true;
                        }
                        i += 1;
                        continue;
                    }
                }
                i += 1;
                continue;
            }
            if arg == "-" {
                // Reading from stdin — do not intercept
                return None;
            }
            files.push(arg.to_string());
            i += 1;
        }

        // Plain `head file` or `tail file` without flags defaults to 10 lines in POSIX — always small!
        if is_head_or_tail && tokens.len() == idx + 1 + files.len() {
            is_bounded_small = true;
        }

        if files.is_empty() {
            None
        } else {
            Some((bin, is_bounded_small, files))
        }
    }

    fn is_small_line_bound(raw: &str) -> bool {
        let trimmed = raw.trim();
        // Leading `+N` in `tail -n +10` means "from line 10 to EOF" (unbounded!)
        if trimmed.starts_with('+') {
            return false;
        }
        trimmed
            .trim_start_matches('-')
            .parse::<usize>()
            .is_ok_and(|n| n <= crate::constants::READ_FILE_UNBOUNDED_THRESHOLD)
    }

    fn is_small_byte_bound(raw: &str, byte_ceiling: usize) -> bool {
        let trimmed = raw.trim();
        if trimmed.starts_with('+') {
            return false;
        }
        trimmed
            .trim_start_matches('-')
            .parse::<usize>()
            .is_ok_and(|n| n <= byte_ceiling)
    }

    fn evaluate_with_ceiling(ctx: &ToolContext<'_>, byte_ceiling: usize) -> AdmissionDecision {
        if !matches!(ctx.tool_name, "exec_cmd" | "sandbox_exec") {
            return AdmissionDecision::Allow;
        }
        let Some(cmd) = crate::tools::param::get_str_with_aliases(ctx.args, &["command", "cmd"])
        else {
            return AdmissionDecision::Allow;
        };
        let Some(tokens) = Self::tokenize_simple_unfiltered_command(cmd) else {
            return AdmissionDecision::Allow;
        };
        let Some((bin, is_bounded_small, files)) =
            Self::inspect_dump_command(&tokens, byte_ceiling)
        else {
            return AdmissionDecision::Allow;
        };
        if is_bounded_small {
            return AdmissionDecision::Allow;
        }

        // Resolve candidate file sizes in O(1) via metadata() BEFORE reading or spawning a shell
        let base_dir = crate::tools::param::get_str_with_aliases(ctx.args, &["cwd", "working_dir"])
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != ".")
            .map(|c| ctx.workspace_root.join(c))
            .unwrap_or_else(|| ctx.workspace_root.to_path_buf());

        let mut resolved_rel_paths: Vec<String> = Vec::with_capacity(files.len());
        let mut total_bytes: u64 = 0;
        let Ok(canon_root) = std::fs::canonicalize(ctx.workspace_root) else {
            return AdmissionDecision::Allow;
        };

        for raw_file in &files {
            let candidate = if Path::new(raw_file).is_absolute() {
                std::path::PathBuf::from(raw_file)
            } else {
                base_dir.join(raw_file)
            };
            let Ok(validated) =
                crate::sandbox::path::validate_path_in_workspace(ctx.workspace_root, &candidate)
            else {
                return AdmissionDecision::Allow;
            };
            let Ok(meta) = std::fs::metadata(&validated) else {
                return AdmissionDecision::Allow;
            };
            if !meta.is_file() {
                return AdmissionDecision::Allow;
            }
            total_bytes = total_bytes.saturating_add(meta.len());
            let rel = validated
                .strip_prefix(&canon_root)
                .unwrap_or(Path::new(raw_file))
                .to_string_lossy()
                .replace('\\', "/");
            resolved_rel_paths.push(rel);
        }

        if total_bytes <= byte_ceiling as u64 {
            return AdmissionDecision::Allow;
        }

        let start = std::time::Instant::now();
        let redirected = if resolved_rel_paths.len() == 1 {
            crate::tools::fs::read_file_with_options(
                ctx.workspace_root,
                &resolved_rel_paths[0],
                None,
                None,
                false,
            )
        } else {
            crate::tools::fs::batch_read(ctx.workspace_root, &resolved_rel_paths, None)
        };

        let Ok(body) = redirected else {
            return AdmissionDecision::Allow;
        };

        let header = format!(
            "[Admission Control: Intercepted unfiltered shell `{bin}` dump (~{} tokens / {} bytes > dynamic ceiling {} tokens). Returned clamped view + AST symbol outline in 0 extra turns — use `read_file(path, start_line, end_line)` or `read_file(path, query)` for targeted spans.]\n\n",
            total_bytes / 4,
            total_bytes,
            byte_ceiling / 4
        );

        tracing::info!(
            tool = ctx.tool_name,
            bin = %bin,
            total_bytes,
            byte_ceiling,
            files = ?resolved_rel_paths,
            "ReadAdmissionMiddleware short-circuited oversized unfiltered shell read"
        );

        AdmissionDecision::ShortCircuit(ToolResult {
            tool_id: String::new(),
            tool_name: ctx.tool_name.to_string(),
            success: true,
            output: format!("{header}{body}"),
            display_output: String::new(),
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
}

impl ToolMiddleware for ReadAdmissionMiddleware {
    fn before(&self, ctx: &ToolContext<'_>) -> AdmissionDecision {
        let byte_ceiling = EconomicAdmissionPolicy::current_byte_ceiling(ctx.workspace_root);
        Self::evaluate_with_ceiling(ctx, byte_ceiling)
    }
}

/// Emits a `tracing::debug!` span with tool name, success flag, and wall-clock
/// duration so every execution appears in structured logs.
pub struct TimingMiddleware;

impl ToolMiddleware for TimingMiddleware {
    fn after(&self, ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        tracing::debug!(
            tool = ctx.tool_name,
            success = result.success,
            duration_ms = result.duration_ms,
            output_bytes = result.output.len(),
            "tool executed"
        );
        result
    }
}

/// Scrubs secrets from tool output and display diff using the global `SecretRedactor`.
///
/// This is the canonical, single redaction hook — the ad-hoc inline call in
/// `agent/loop.rs` delegates here via `ToolPipeline::run`.
pub struct RedactMiddleware;

impl ToolMiddleware for RedactMiddleware {
    fn after(&self, _ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        let redactor = crate::sandbox::redact::SecretRedactor::global();
        let output = redactor.redact(&result.output);
        let display_output = if result.display_output.is_empty() {
            result.display_output
        } else {
            redactor.redact(&result.display_output)
        };
        ToolResult {
            output,
            display_output,
            ..result
        }
    }
}

/// Automatically persists large tool outputs (> 200 lines and > 8 KiB) to
/// `.minicode/tmp/tool_outputs/` before `SmartDonutTruncator` slices the middle,
/// appending a 1-line pointer so the Orchestrator can inspect omitted lines on demand.
pub struct SpilloverMiddleware;

impl ToolMiddleware for SpilloverMiddleware {
    fn after(&self, ctx: &ToolContext<'_>, mut result: ToolResult) -> ToolResult {
        if result.output.len() <= 8192
            || result.output.contains("tmp/tool_outputs/")
            || result.output.contains("logs/last_exec.log")
        {
            return result;
        }
        let line_count = result.output.lines().count();
        if line_count <= 200 {
            return result;
        }
        let byte_len = result.output.len();
        if let Some(rel_path) =
            save_spillover_output(ctx.workspace_root, ctx.tool_name, &result.output)
        {
            result.output.push_str(&format!(
                "\n[Full untruncated output ({} lines, {}B) saved to {} — use read_file or grep_search on this path if middle lines are omitted]",
                line_count, byte_len, rel_path
            ));
        }
        result
    }
}

/// Emits a structured `tracing::info!` event whenever a destructive (file-
/// modifying) tool completes. Provides an immutable audit trail without
/// blocking execution.
pub struct CheckpointMiddleware;

impl CheckpointMiddleware {
    /// Resolves the target file path or multi-file scope (`glob`, `pattern`, `directory`)
    /// for structured audit logging so multi-file tools like `replace_in_files` never log `<unknown>`.
    pub fn resolve_checkpoint_target(args: &serde_json::Value) -> &str {
        crate::tools::param::get_str_with_aliases(args, FILE_PATH_ALIASES)
            .or_else(|| crate::tools::param::get_str_with_aliases(args, MUTATION_SCOPE_ALIASES))
            .unwrap_or("<unknown>")
    }
}

impl ToolMiddleware for CheckpointMiddleware {
    fn after(&self, ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        if matches!(ctx.tool_name, "exec_cmd" | "sandbox_exec") {
            if let Some(cmd) =
                crate::tools::param::get_str_with_aliases(ctx.args, &["command", "cmd"])
            {
                crate::agent::verification_barrier::SessionQualityTelemetry::record_command_execution(
                    ctx.workspace_root,
                    cmd,
                    &result.output,
                    result.success,
                );
            }
        }
        if crate::tools::meta::is_file_mutation(ctx.tool_name) && result.success {
            let file_path = Self::resolve_checkpoint_target(ctx.args);
            tracing::info!(
                tool = ctx.tool_name,
                file = file_path,
                success = result.success,
                duration_ms = result.duration_ms,
                "destructive tool completed — checkpoint logged"
            );
        }
        result
    }
}

/// Appends an inline unified diff block to the tool output for `write_file`
/// and `patch_file` results.
pub struct DiffMiddleware;

/// Marker prefix embedded in tool output to signal a diff block to the TUI.
pub const DIFF_MARKER: &str = "MINICODE_DIFF_BLOCK:";

impl ToolMiddleware for DiffMiddleware {
    fn after(&self, ctx: &ToolContext<'_>, mut result: ToolResult) -> ToolResult {
        // Only apply to successful file-modifying tools
        if !result.success || !crate::tools::meta::is_file_mutation(ctx.tool_name) {
            return result;
        }

        let Some(file_path) =
            crate::tools::param::get_str_with_aliases(ctx.args, FILE_PATH_ALIASES)
        else {
            return result;
        };

        // Resolve full path for reading new content
        let full_path = ctx.workspace_root.join(file_path);
        let file_after = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    "DiffMiddleware: could not read '{}' after write: {}",
                    file_path,
                    e
                );
                return result;
            }
        };

        let before = ctx.file_before.unwrap_or("");
        let diff_lines = crate::tools::diff::compute_diff(before, &file_after);
        if !crate::tools::diff::has_changes(&diff_lines) {
            tracing::debug!("DiffMiddleware: no changes detected for '{}'", file_path);
            return result;
        }

        let edit_report =
            crate::agent::verification_barrier::SessionQualityTelemetry::record_file_edit(
                ctx.workspace_root,
                &full_path,
                before,
                &file_after,
            );
        if let Some(badge) = edit_report.format_telemetry_badge() {
            result.output.push('\n');
            result.output.push_str(&badge);
        }

        let diff_text = crate::tools::diff::format_diff_plain(&diff_lines, file_path);
        let display_output = format!("{}{}\n{}", DIFF_MARKER, diff_text, result.output);
        tracing::debug!(
            tool = ctx.tool_name,
            file = file_path,
            "DiffMiddleware: attached diff ({} diff lines)",
            diff_lines.len()
        );
        ToolResult {
            display_output,
            ..result
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_result(output: &str, success: bool) -> ToolResult {
        ToolResult {
            tool_id: "t1".into(),
            tool_name: "test_tool".into(),
            success,
            output: output.to_string(),
            display_output: String::new(),
            duration_ms: 42,
        }
    }

    #[test]
    fn test_redact_middleware_strips_openai_key() {
        let mw = RedactMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "exec_cmd",
            workspace_root: Path::new("."),
            args: &args,
            file_before: None,
        };
        let result = make_result("key=sk-proj-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", true);
        let out = mw.after(&ctx, result);
        assert!(out.output.contains("[REDACTED]"));
        assert!(!out.output.contains("sk-proj-"));
    }

    #[test]
    fn test_redact_middleware_passthrough_clean_output() {
        let mw = RedactMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "read_file",
            workspace_root: Path::new("."),
            args: &args,
            file_before: None,
        };
        let result = make_result("fn main() { println!(\"hello\"); }", true);
        let out = mw.after(&ctx, result);
        assert_eq!(out.output, "fn main() { println!(\"hello\"); }");
    }

    #[test]
    fn test_timing_middleware_is_transparent() {
        let mw = TimingMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "read_file",
            workspace_root: Path::new("."),
            args: &args,
            file_before: None,
        };
        let result = make_result("some output", true);
        let out = mw.after(&ctx, result.clone());
        assert_eq!(out.output, result.output);
        assert_eq!(out.duration_ms, result.duration_ms);
    }

    #[test]
    fn test_checkpoint_middleware_passthrough_non_destructive() {
        let mw = CheckpointMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "read_file",
            workspace_root: Path::new("."),
            args: &args,
            file_before: None,
        };
        let result = make_result("file contents", true);
        let out = mw.after(&ctx, result.clone());
        assert_eq!(out.output, result.output);
    }

    #[test]
    fn test_checkpoint_middleware_logs_write_file() {
        let mw = CheckpointMiddleware;
        let args = json!({"path": "src/main.rs"});
        let ctx = ToolContext {
            tool_name: "write_file",
            workspace_root: Path::new("."),
            args: &args,
            file_before: None,
        };
        let result = make_result("written 512 bytes", true);
        let out = mw.after(&ctx, result.clone());
        assert_eq!(out.output, result.output);
    }

    #[test]
    fn test_pipeline_default_applies_all_middlewares() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let result = make_result(
            "token: sk-proj-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA — done",
            true,
        );
        let out = pipeline.run(result, "exec_cmd", Path::new("."), &args, None);
        assert!(out.output.contains("[REDACTED]"));
        assert!(!out.output.contains("sk-proj-"));
    }

    #[test]
    fn test_pipeline_preserves_failure_flag() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let result = make_result("command not found", false);
        let out = pipeline.run(result, "exec_cmd", Path::new("."), &args, None);
        assert!(!out.success);
    }

    #[test]
    fn test_pipeline_preserves_duration_ms() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let mut result = make_result("ok", true);
        result.duration_ms = 1337;
        let out = pipeline.run(result, "read_file", Path::new("."), &args, None);
        assert_eq!(out.duration_ms, 1337);
    }

    #[test]
    fn test_empty_pipeline_is_identity() {
        let pipeline = ToolPipeline::new(vec![]);
        let args = json!({});
        let result = make_result("raw output", true);
        let out = pipeline.run(result.clone(), "read_file", Path::new("."), &args, None);
        assert_eq!(out.output, result.output);
        assert_eq!(out.success, result.success);
    }

    #[test]
    fn test_checkpoint_middleware_resolves_glob_and_pattern_scopes() {
        let glob_args = json!({
            "glob": "src/components/**/*.tsx",
            "search": "oldToken",
            "replace": "newToken"
        });
        assert_eq!(
            CheckpointMiddleware::resolve_checkpoint_target(&glob_args),
            "src/components/**/*.tsx"
        );

        let single_args = json!({"path": "src/App.tsx"});
        assert_eq!(
            CheckpointMiddleware::resolve_checkpoint_target(&single_args),
            "src/App.tsx"
        );

        let empty_args = json!({});
        assert_eq!(
            CheckpointMiddleware::resolve_checkpoint_target(&empty_args),
            "<unknown>"
        );
    }

    #[test]
    fn test_economic_admission_policy_scales_with_context_and_pricing() {
        // High context pressure tightens ceiling quadratically
        let fresh_ceiling =
            EconomicAdmissionPolicy::compute_admission_token_ceiling(128_000, 0, 0.0, None, None);
        let pressured_ceiling = EconomicAdmissionPolicy::compute_admission_token_ceiling(
            128_000, 90_000, 0.0, None, None,
        );
        assert!(
            pressured_ceiling < fresh_ceiling,
            "High utilization must tighten admission ceiling ({pressured_ceiling} vs {fresh_ceiling})"
        );
        assert!(pressured_ceiling >= 1_500);

        // Large main-to-fast price gap (e.g. Claude Sonnet vs Haiku or Gemini Pro vs Flash)
        // lowers the break-even ceiling below raw context pressure
        let with_fast_shunt = EconomicAdmissionPolicy::compute_admission_token_ceiling(
            200_000,
            0,
            3.00,
            Some(0.80),
            Some(4.00),
        );
        let without_fast =
            EconomicAdmissionPolicy::compute_admission_token_ceiling(200_000, 0, 3.00, None, None);
        assert!(
            with_fast_shunt < without_fast,
            "Cheaper Fast tier should tighten break-even ceiling ({with_fast_shunt} vs {without_fast})"
        );
    }

    #[test]
    fn test_read_admission_middleware_intercepts_unbounded_cat_and_allows_piped_or_bounded() {
        let ws = tempfile::tempdir().expect("tempdir");
        let big_file = ws.path().join("big.rs");
        let mut content = String::new();
        for i in 0..400 {
            content.push_str(&format!(
                "pub fn symbol_fn_{i}(v: usize) -> usize {{ v + {i} }}\n"
            ));
        }
        std::fs::write(&big_file, &content).expect("write big.rs");

        // 1. Oversized unfiltered `cat big.rs` -> short-circuited in 0 extra turns with AST outline
        let cat_args = json!({ "command": "cat big.rs" });
        let ctx_cat = ToolContext {
            tool_name: "exec_cmd",
            workspace_root: ws.path(),
            args: &cat_args,
            file_before: None,
        };
        match ReadAdmissionMiddleware::evaluate_with_ceiling(&ctx_cat, 4096) {
            AdmissionDecision::ShortCircuit(res) => {
                assert!(res.success);
                assert!(res.output.contains("Admission Control"));
                assert!(res.output.contains("AST Outline"));
            }
            AdmissionDecision::Allow => {
                panic!("Oversized unfiltered `cat big.rs` must be intercepted by ReadAdmissionMiddleware")
            }
        }

        // 2. Piped targeted read (`cat big.rs | grep symbol_fn_399`) -> allowed straight through
        let piped_args = json!({ "command": "cat big.rs | grep symbol_fn_399" });
        let ctx_piped = ToolContext {
            tool_name: "exec_cmd",
            workspace_root: ws.path(),
            args: &piped_args,
            file_before: None,
        };
        assert!(matches!(
            ReadAdmissionMiddleware::evaluate_with_ceiling(&ctx_piped, 4096),
            AdmissionDecision::Allow
        ));

        // 3. Explicitly bounded `head -n 30 big.rs` -> allowed straight through
        let bounded_args = json!({ "command": "head -n 30 big.rs" });
        let ctx_bounded = ToolContext {
            tool_name: "exec_cmd",
            workspace_root: ws.path(),
            args: &bounded_args,
            file_before: None,
        };
        assert!(matches!(
            ReadAdmissionMiddleware::evaluate_with_ceiling(&ctx_bounded, 4096),
            AdmissionDecision::Allow
        ));

        // 4. Unbounded `tail -n +1 big.rs` -> intercepted!
        let unbounded_tail = json!({ "command": "tail -n +1 big.rs" });
        let ctx_tail = ToolContext {
            tool_name: "exec_cmd",
            workspace_root: ws.path(),
            args: &unbounded_tail,
            file_before: None,
        };
        assert!(matches!(
            ReadAdmissionMiddleware::evaluate_with_ceiling(&ctx_tail, 4096),
            AdmissionDecision::ShortCircuit(_)
        ));
    }
}
