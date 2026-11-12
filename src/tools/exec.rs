use crate::error::{Result, ToolError};
use crate::sandbox::env::build_sanitized_command;
#[cfg(target_os = "linux")]
use crate::sandbox::landlock::apply_landlock_sandbox;
use std::path::Path;
use std::time::Duration;

/// Resolves the context window token limit for tool execution.
/// First checks the global active context limit, then environment variables, then workspace configuration.
pub fn resolve_context_window(workspace_root: &Path) -> usize {
    let active = crate::context::budget::donut::get_active_context_limit();
    if active > 0 {
        return active;
    }

    if let Ok(val) = std::env::var("MINICODE_CONTEXT_WINDOW") {
        if let Ok(parsed) = val.parse::<usize>() {
            return parsed;
        }
    }

    if let Ok(model) = std::env::var("MINICODE_MODEL") {
        return crate::agent::models::get_model_context_limit(&model);
    }

    // Check workspace-local config.toml if it exists
    let local_cfg = workspace_root
        .join(crate::constants::WORKSPACE_DIR_NAME)
        .join(crate::constants::CONFIG_FILE_NAME);
    if local_cfg.exists() {
        if let Ok(content) = std::fs::read_to_string(&local_cfg) {
            if let Ok(val) = toml::from_str::<serde_json::Value>(&content) {
                if let Some(ctx) = val
                    .get("provider")
                    .and_then(|p| p.get("context_window"))
                    .and_then(|c| c.as_u64())
                {
                    return ctx as usize;
                }
                if let Some(model) = val
                    .get("provider")
                    .and_then(|p| p.get("model"))
                    .and_then(|m| m.as_str())
                {
                    return crate::agent::models::get_model_context_limit(model);
                }
            }
        }
    }

    0
}

static CONFIGURED_EXEC_TIMEOUT_SECS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(crate::constants::EXEC_DEFAULT_TIMEOUT_SECS);

/// Updates the configured base execution timeout from `config.agent.timeout` or CLI `--timeout`.
pub fn set_configured_exec_timeout(secs: u64) {
    CONFIGURED_EXEC_TIMEOUT_SECS.store(secs.max(5), std::sync::atomic::Ordering::Relaxed);
}

/// Returns the active configured base execution timeout in seconds.
pub fn get_configured_exec_timeout() -> u64 {
    CONFIGURED_EXEC_TIMEOUT_SECS
        .load(std::sync::atomic::Ordering::Relaxed)
        .max(5)
}

/// Determines the optimal execution timeout dynamically based on command characteristics,
/// environment configurations, and command workload intensity.
///
/// Prevents premature timeout when an LLM echoes schema defaults (e.g. 30s) on heavy build or install tasks,
/// while dynamically scaling time based on environment settings and hardware concurrency.
pub fn resolve_smart_exec_timeout(command_str: &str, explicit_timeout: Option<u64>) -> Duration {
    // 1. Dynamic base timeout from environment, runtime config.toml / CLI (-t), or default
    let env_timeout = std::env::var("MINICODE_CMD_TIMEOUT")
        .or_else(|_| std::env::var("MINICODE_TIMEOUT"))
        .ok()
        .and_then(|v| v.parse::<u64>().ok());

    let base_timeout_secs = env_timeout.unwrap_or_else(get_configured_exec_timeout);

    // 2. Classify command intensity and compute dynamic duration
    let cmd = command_str.trim().to_lowercase();
    let (dynamic_floor_secs, is_heavy) = compute_dynamic_command_floor(&cmd, base_timeout_secs);

    // 3. Resolve with explicit timeout (if provided by LLM or caller)
    if let Some(explicit) = explicit_timeout {
        if is_heavy {
            // If heavy task and caller passed a low explicit timeout (e.g. schema default 30s or lower),
            // dynamically elevate to dynamic_floor_secs.
            // If caller explicitly asked for more time (e.g. 300s), respect their choice.
            Duration::from_secs(std::cmp::max(explicit, dynamic_floor_secs))
        } else {
            // For standard lightweight commands, respect whatever explicit timeout the caller requested
            Duration::from_secs(explicit)
        }
    } else {
        Duration::from_secs(dynamic_floor_secs)
    }
}

/// Semantic workload category derived by token-stream analysis of a shell command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandWorkload {
    /// Fast inspection, version/help flag, or piped output filter (`ls`, `git status`, `--version`, `| head`).
    InstantProbe,
    /// Persistent server, listener, or file-watching daemon (`--watch`, `dev`, `serve`, `http.server`, trailing `&`).
    ServerDaemon,
    /// Network-bound dependency resolution or package synchronization (`install`, `sync`, `fetch`, `add`, `ci`).
    DependencyInstall,
    /// Compilation, test suite, typecheck, or bundle workload (`build`, `test`, `check`, `compile`, `pytest`, `tsc`).
    BuildOrTest,
    /// General script, runner, or multi-step automation task (`bash script.sh`, `python run.py`, `cargo run`).
    ManagedTask,
}

/// Structural token-stream profile of an arbitrary shell command across any language or stack.
#[derive(Debug, Clone)]
pub struct CommandProfile {
    pub program: String,
    pub action: Option<String>,
    pub task_slug: String,
    pub workload: CommandWorkload,
    pub port_hint: Option<u16>,
}

impl CommandProfile {
    const WRAPPERS: &'static [&'static str] = &[
        "nice", "nohup", "time", "env", "sudo", "stdbuf", "timeout", "chrt", "ionice", "taskset",
    ];

    const INSTANT_UTILITIES: &'static [&'static str] = &[
        "cd", "pushd", "popd", "export", "source", "unset", "set", "alias", "ulimit", "umask",
        "echo", "printf", "pwd", "ls", "cat", "head", "tail", "grep", "rg", "find", "which",
        "whereis", "uname", "whoami", "id", "date", "stat", "wc", "tr", "cut", "sort", "uniq",
        "sed", "awk", "perl", "jq", "diff", "file", "realpath", "dirname", "basename", "readlink",
        "printenv", "type", "command", "hash", "ps", "pgrep", "kill", "pkill", "killall", "lsof",
        "ss", "netstat", "fuser", "mkdir", "rm", "cp", "mv", "touch", "chmod", "chown", "ln",
        "true", "false", "test", "sleep", "curl", "wget", "git", "gh",
    ];

    const FILLER_SUBCOMMANDS: &'static [&'static str] = &["run", "exec", "task", "x", "pip", "cmd"];

    const INSTALL_ACTIONS: &'static [&'static str] = &[
        "install", "i", "ci", "add", "sync", "fetch", "download", "restore", "update", "upgrade",
    ];

    const BUILD_TEST_ACTIONS: &'static [&'static str] = &[
        "build",
        "compile",
        "test",
        "check",
        "clippy",
        "bench",
        "bundle",
        "release",
        "package",
        "verify",
        "audit",
        "lint",
        "typecheck",
    ];

    const BUILD_TEST_PROGRAMS: &'static [&'static str] = &[
        "pytest",
        "vitest",
        "jest",
        "rspec",
        "phpunit",
        "mypy",
        "pyright",
        "ruff",
        "tsc",
        "rustc",
        "gcc",
        "g++",
        "clang",
        "clang++",
        "cmake",
        "ninja",
        "make",
        "golangci-lint",
    ];

    const SERVER_ACTIONS: &'static [&'static str] = &[
        "dev",
        "serve",
        "server",
        "preview",
        "watch",
        "listen",
        "up",
        "http-server",
        "simplehttpserver",
    ];

    const SERVER_PROGRAMS: &'static [&'static str] = &[
        "uvicorn",
        "gunicorn",
        "http-server",
        "vite",
        "next",
        "astro",
        "remix",
    ];

    const SERVER_FLAGS: &'static [&'static str] =
        &["--watch", "--reload", "--live-reload", "--hot", "--serve"];

    fn sanitize_slug(raw: &str) -> String {
        let stem = Path::new(raw)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(raw);
        let mut out = String::with_capacity(stem.len());
        let mut last_dash = false;
        for ch in stem.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                last_dash = false;
            } else if !last_dash && !out.is_empty() {
                out.push('-');
                last_dash = true;
            }
        }
        out.trim_matches('-').to_string()
    }

    /// Parses a shell heredoc delimiter after `<<` (or `<<-`), returning `(unquoted_delimiter, strip_leading_tabs, next_byte_idx)`.
    fn parse_heredoc_delimiter(cmd: &str, after_lt_lt: usize) -> Option<(String, bool, usize)> {
        let bytes = cmd.as_bytes();
        let len = bytes.len();
        let mut pos = after_lt_lt;
        let mut strip_tabs = false;
        if pos < len && bytes[pos] == b'-' {
            strip_tabs = true;
            pos += 1;
        }
        while pos < len && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        if pos >= len || bytes[pos] == b'\n' {
            return None;
        }

        let mut delim = String::new();
        let mut q_single = false;
        let mut q_double = false;
        while pos < len {
            let ch = cmd[pos..].chars().next()?;
            let ch_len = ch.len_utf8();
            if q_single {
                if ch == '\'' {
                    q_single = false;
                } else {
                    delim.push(ch);
                }
                pos += ch_len;
                continue;
            }
            if q_double {
                if ch == '"' {
                    q_double = false;
                } else if ch == '\\' && pos + ch_len < len {
                    let next_ch = cmd[pos + ch_len..].chars().next()?;
                    delim.push(next_ch);
                    pos += ch_len + next_ch.len_utf8();
                    continue;
                } else {
                    delim.push(ch);
                }
                pos += ch_len;
                continue;
            }
            match ch {
                '\'' => {
                    q_single = true;
                    pos += ch_len;
                }
                '"' => {
                    q_double = true;
                    pos += ch_len;
                }
                '\\' if pos + ch_len < len => {
                    let next_ch = cmd[pos + ch_len..].chars().next()?;
                    delim.push(next_ch);
                    pos += ch_len + next_ch.len_utf8();
                }
                ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | '<' | '>' | '(' | ')' => break,
                _ => {
                    delim.push(ch);
                    pos += ch_len;
                }
            }
        }

        if delim.is_empty() {
            None
        } else {
            Some((delim, strip_tabs, pos))
        }
    }

    /// Consumes queued heredoc bodies starting right after a `\n` (at `body_start`), returning the index
    /// immediately after the final delimiter line (at its trailing `\n` or `cmd.len()`).
    fn consume_heredoc_bodies(
        cmd: &str,
        mut pos: usize,
        pending: &mut Vec<(String, bool)>,
    ) -> usize {
        let len = cmd.len();
        for (delim, strip_tabs) in pending.drain(..) {
            while pos < len {
                let line_end = cmd[pos..].find('\n').map(|rel| pos + rel).unwrap_or(len);
                let raw_line = cmd[pos..line_end]
                    .strip_suffix('\r')
                    .unwrap_or(&cmd[pos..line_end]);
                let candidate = if strip_tabs {
                    raw_line.trim_start_matches('\t')
                } else {
                    raw_line
                };
                pos = line_end;
                if candidate == delim {
                    break;
                }
                if pos < len && cmd.as_bytes()[pos] == b'\n' {
                    pos += 1;
                }
            }
        }
        pos
    }

    /// Splits a shell command string into top-level `(segment, terminated_by_bg_amp)` pairs separated by
    /// `;`, `\n`, `&&`, `||`, or standalone `&` while respecting single/double quotes, backslash escapes,
    /// subshell/brace grouping `(...)` / `{...}`, heredoc bodies (`<<EOF` / `<<-EOF`), and file-descriptor redirections.
    fn split_shell_segments(cmd: &str) -> Vec<(&str, bool)> {
        let bytes = cmd.as_bytes();
        let len = bytes.len();
        let mut segments = Vec::new();
        let mut start = 0usize;
        let mut i = 0usize;
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        let mut group_depth = 0usize;
        let mut pending_heredocs: Vec<(String, bool)> = Vec::new();

        while i < len {
            let b = bytes[i];
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if b == b'\\' && !in_single {
                escaped = true;
                i += 1;
                continue;
            }
            if b == b'\'' && !in_double {
                in_single = !in_single;
                i += 1;
                continue;
            }
            if b == b'"' && !in_single {
                in_double = !in_double;
                i += 1;
                continue;
            }
            if !in_single && !in_double {
                // Detect heredoc operator `<<` (excluding here-string `<<<`)
                if b == b'<'
                    && i + 1 < len
                    && bytes[i + 1] == b'<'
                    && (i + 2 >= len || bytes[i + 2] != b'<')
                {
                    if let Some((delim, strip_tabs, next_i)) =
                        Self::parse_heredoc_delimiter(cmd, i + 2)
                    {
                        pending_heredocs.push((delim, strip_tabs));
                        i = next_i;
                        continue;
                    }
                }
                if b == b'(' || b == b'{' {
                    group_depth += 1;
                    i += 1;
                    continue;
                }
                if b == b')' || b == b'}' {
                    group_depth = group_depth.saturating_sub(1);
                    i += 1;
                    continue;
                }
                if b == b'\n' && !pending_heredocs.is_empty() {
                    let after_body =
                        Self::consume_heredoc_bodies(cmd, i + 1, &mut pending_heredocs);
                    if group_depth == 0 {
                        let seg = cmd[start..after_body].trim();
                        if !seg.is_empty() {
                            segments.push((seg, false));
                        }
                        i = if after_body < len && bytes[after_body] == b'\n' {
                            after_body + 1
                        } else {
                            after_body
                        };
                        start = i;
                    } else {
                        i = if after_body < len && bytes[after_body] == b'\n' {
                            after_body + 1
                        } else {
                            after_body
                        };
                    }
                    continue;
                }
                if group_depth == 0 {
                    if b == b';' || b == b'\n' {
                        let seg = cmd[start..i].trim();
                        if !seg.is_empty() {
                            segments.push((seg, false));
                        }
                        i += 1;
                        start = i;
                        continue;
                    }
                    if (b == b'&' && i + 1 < len && bytes[i + 1] == b'&')
                        || (b == b'|' && i + 1 < len && bytes[i + 1] == b'|')
                    {
                        let seg = cmd[start..i].trim();
                        if !seg.is_empty() {
                            segments.push((seg, false));
                        }
                        i += 2;
                        start = i;
                        continue;
                    }
                    if b == b'&' {
                        let prev_is_redirect =
                            i > 0 && (bytes[i - 1] == b'>' || bytes[i - 1] == b'<');
                        let next_is_redirect = i + 1 < len && bytes[i + 1] == b'>';
                        if !prev_is_redirect && !next_is_redirect {
                            let seg = cmd[start..i].trim();
                            if !seg.is_empty() {
                                segments.push((seg, true));
                            }
                            i += 1;
                            start = i;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }

        if start < len {
            let seg = cmd[start..len].trim();
            if !seg.is_empty() {
                segments.push((seg, false));
            }
        }
        segments
    }

    /// Returns true if `seg` is a self-detaching subshell `( ... & )` that returns immediately to the parent shell.
    fn is_detached_subshell(seg: &str) -> bool {
        let t = seg.trim();
        if t.starts_with('(') && t.ends_with(')') && t.len() >= 3 {
            let inner = t[1..t.len() - 1].trim();
            return inner.ends_with('&') && !inner.ends_with("&&") && !inner.ends_with(">&");
        }
        false
    }

    /// Parses a shell command pipeline into a normalized `CommandProfile` without hardcoded command strings.
    pub fn analyze(command_str: &str) -> Self {
        let trimmed = command_str.trim();
        let top_segments = Self::split_shell_segments(trimmed);
        let has_trailing_amp = top_segments
            .last()
            .is_some_and(|&(seg, bg)| bg && !Self::is_detached_subshell(seg));
        let clean = if has_trailing_amp {
            trimmed.trim_end_matches('&').trim()
        } else {
            trimmed
        };

        // Detect trailing pipe into bounded output filter (| head, | tail, | wc, | grep, | cat)
        let has_pipe_pager = clean
            .rsplit_once('|')
            .and_then(|(_, rhs)| rhs.split_whitespace().next())
            .is_some_and(|pager| {
                let p = Self::sanitize_slug(pager);
                matches!(
                    p.as_str(),
                    "head" | "tail" | "wc" | "grep" | "rg" | "cat" | "sed" | "awk" | "jq"
                )
            });

        let mut selected_program = String::new();
        let mut selected_action: Option<String> = None;
        let mut selected_inline_eval = false;
        let mut selected_is_final_fg = true;
        let mut has_meta_flag = false;
        let mut has_server_flag = false;
        let mut port_hint: Option<u16> = None;

        let segments = Self::split_shell_segments(clean);
        // Identify the index of the last foreground (non-detached) segment
        let last_fg_idx = segments
            .iter()
            .rposition(|&(s, bg)| !bg && !Self::is_detached_subshell(s));

        for (seg_idx, (raw_seg, is_bg)) in segments.iter().copied().enumerate() {
            // Skip backgrounded mid-pipeline segments (`cmd & next_cmd`) and detached subshells (`(cmd &)`)
            // because the shell detaches them immediately and continues executing foreground stages.
            if Self::is_detached_subshell(raw_seg)
                || (is_bg && Some(seg_idx) != last_fg_idx && seg_idx + 1 < segments.len())
            {
                continue;
            }
            let unwrapped = raw_seg
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or(raw_seg);
            // If this segment embeds a multi-line heredoc (`<<EOF\n...\nEOF`), inspect only the command header line
            // so heredoc body tokens (`|`, strings, script statements) never pollute command classification.
            let cmd_header = if unwrapped.contains("<<") && unwrapped.contains('\n') {
                unwrapped.lines().next().unwrap_or(unwrapped)
            } else {
                unwrapped
            };
            let seg = cmd_header
                .split_once('|')
                .map(|(lhs, _)| lhs)
                .unwrap_or(cmd_header)
                .trim();
            if seg.is_empty() {
                continue;
            }
            let tokens: Vec<&str> = seg.split_whitespace().collect();
            let mut idx = 0;

            // Skip leading ENV=VAL assignments, POSIX shell clause prefixes (do, then, else, while, if),
            // and execution wrappers (nice -n 19, timeout 10, sudo, etc.)
            while idx < tokens.len() {
                let tok = tokens[idx];
                if tok.contains('=') && !tok.starts_with('-') {
                    idx += 1;
                    continue;
                }
                if matches!(
                    tok,
                    "do" | "then" | "else" | "elif" | "while" | "until" | "if" | "!"
                ) {
                    idx += 1;
                    continue;
                }
                let slug = Self::sanitize_slug(tok);
                if Self::WRAPPERS.contains(&slug.as_str()) {
                    idx += 1;
                    while idx < tokens.len() {
                        let next_tok = tokens[idx];
                        if next_tok.starts_with('-') {
                            idx += 1;
                            if idx < tokens.len()
                                && tokens[idx]
                                    .chars()
                                    .all(|c| c.is_ascii_digit() || c == '.' || c == 's')
                            {
                                idx += 1;
                            }
                        } else if next_tok
                            .chars()
                            .all(|c| c.is_ascii_digit() || c == '.' || c == 's')
                        {
                            idx += 1;
                        } else {
                            break;
                        }
                    }
                    continue;
                }
                break;
            }

            if idx >= tokens.len() {
                continue;
            }

            // Pure POSIX shell loop/branch header or closer clauses execute no external binary on their own
            if matches!(
                tokens[idx],
                "for" | "select" | "case" | "done" | "fi" | "esac" | "{" | "}"
            ) {
                continue;
            }

            if tokens[idx] == "." {
                selected_program = "source".to_string();
                selected_action = None;
                continue;
            }

            let prog_slug = Self::sanitize_slug(tokens[idx]);
            if prog_slug.is_empty() {
                continue;
            }

            let mut seg_action: Option<String> = None;
            let mut seg_meta_flag = false;
            let mut seg_inline_eval = false;
            let mut seg_has_heredoc = false;
            let mut seg_server_flag = false;
            let mut j = idx + 1;
            while j < tokens.len() {
                let tok = tokens[j];
                // Skip shell file-descriptor redirections and heredoc markers (>, >>, <, <<, <<-, 2>&1, &>)
                if tok.contains('>') || tok.contains('<') {
                    if tok.contains("<<") && !tok.contains("<<<") {
                        seg_has_heredoc = true;
                    }
                    if matches!(tok, ">" | ">>" | "<" | "<<" | "<<-" | "&>" | "1>" | "2>") {
                        j += 2;
                    } else {
                        j += 1;
                    }
                    continue;
                }
                let lower_tok = tok.to_ascii_lowercase();
                if matches!(
                    lower_tok.as_str(),
                    "--version" | "--help" | "-v" | "-h" | "version" | "help"
                ) {
                    seg_meta_flag = true;
                }
                let is_combined_eval_short_flag = lower_tok.starts_with('-')
                    && !lower_tok.starts_with("--")
                    && lower_tok.len() >= 2
                    && lower_tok.ends_with('e')
                    && lower_tok[1..].chars().all(|c| c.is_ascii_alphanumeric());
                let is_print_eval_flag = lower_tok == "-p"
                    && matches!(
                        prog_slug.as_str(),
                        "node" | "nodejs" | "bun" | "deno" | "ruby" | "perl"
                    )
                    && tokens
                        .get(j + 1)
                        .is_none_or(|nxt| nxt.parse::<u16>().is_err());
                if matches!(lower_tok.as_str(), "-e" | "-c" | "--eval" | "--print")
                    || is_combined_eval_short_flag
                    || is_print_eval_flag
                {
                    seg_inline_eval = true;
                }
                if Self::SERVER_FLAGS.contains(&lower_tok.as_str()) {
                    seg_server_flag = true;
                }
                if let Ok(port) = tok.parse::<u16>() {
                    if (1024..=65535).contains(&port) {
                        port_hint = port_hint.or(Some(port));
                    }
                }
                if lower_tok == "-m" && j + 1 < tokens.len() {
                    let mod_slug = Self::sanitize_slug(tokens[j + 1]);
                    if !mod_slug.is_empty() {
                        seg_action = Some(mod_slug);
                    }
                    j += 2;
                    continue;
                }
                if !tok.starts_with('-') {
                    let cand = Self::sanitize_slug(tok);
                    if !cand.is_empty()
                        && !Self::FILLER_SUBCOMMANDS.contains(&cand.as_str())
                        && seg_action.is_none()
                    {
                        seg_action = Some(cand);
                    }
                }
                j += 1;
            }

            if seg_has_heredoc && seg_action.is_none() {
                seg_inline_eval = true;
            }

            let seg_is_server = seg_server_flag
                || Self::SERVER_PROGRAMS.contains(&prog_slug.as_str())
                || seg_action
                    .as_deref()
                    .is_some_and(|a| Self::SERVER_ACTIONS.contains(&a));

            selected_program = prog_slug.clone();
            selected_action = seg_action;
            has_meta_flag = seg_meta_flag;
            selected_inline_eval = seg_inline_eval && !seg_is_server;
            selected_is_final_fg = last_fg_idx.is_none_or(|last_idx| seg_idx == last_idx);
            if seg_server_flag {
                has_server_flag = true;
            }
            if !Self::INSTANT_UTILITIES.contains(&prog_slug.as_str()) && !selected_inline_eval {
                break;
            }
        }

        if port_hint.is_none() {
            port_hint = crate::dev::ports::scan_ports_from_output(clean)
                .into_iter()
                .next();
        }

        let prog = selected_program.as_str();
        let act = selected_action.as_deref().unwrap_or("");

        if matches!(act, "http-server" | "simplehttpserver") && port_hint.is_none() {
            port_hint = Some(8000);
        }

        let is_instant = prog.is_empty()
            || has_meta_flag
            || has_pipe_pager
            || selected_inline_eval
            || Self::INSTANT_UTILITIES.contains(&prog);

        let is_install = !is_instant && Self::INSTALL_ACTIONS.contains(&act);
        let is_build_or_test = !is_instant
            && !is_install
            && (Self::BUILD_TEST_ACTIONS.contains(&act)
                || (Self::BUILD_TEST_PROGRAMS.contains(&prog)
                    && !Self::SERVER_ACTIONS.contains(&act)));
        // A compound pipeline is only a resident ServerDaemon if the server command is the terminal
        // foreground stage (or explicitly ends with top-level `&`), not when followed by synchronous stages.
        let is_server = !is_instant
            && !is_install
            && !is_build_or_test
            && selected_is_final_fg
            && (has_trailing_amp
                || has_server_flag
                || Self::SERVER_ACTIONS.contains(&act)
                || (Self::SERVER_PROGRAMS.contains(&prog) && act != "build"));

        let workload = if is_instant {
            CommandWorkload::InstantProbe
        } else if is_install {
            CommandWorkload::DependencyInstall
        } else if is_build_or_test {
            CommandWorkload::BuildOrTest
        } else if is_server {
            CommandWorkload::ServerDaemon
        } else {
            CommandWorkload::ManagedTask
        };

        let normalized_act = match act {
            "i" | "add" => "install",
            other => other,
        };
        let task_slug = if prog.is_empty() {
            "bg-task".to_string()
        } else if !normalized_act.is_empty() && normalized_act != prog {
            format!("{}-{}", prog, normalized_act)
        } else {
            prog.to_string()
        };

        Self {
            program: selected_program,
            action: selected_action,
            task_slug,
            workload,
            port_hint,
        }
    }
}

/// Dynamically calculates the minimum execution timeout based on `CommandProfile` workload semantics
/// and hardware concurrency. Returns `(computed_timeout_seconds, is_heavy_operation)`.
fn compute_dynamic_command_floor(cmd: &str, base_secs: u64) -> (u64, bool) {
    if let Ok(val) = std::env::var("MINICODE_BUILD_TIMEOUT_SECS") {
        if let Ok(custom) = val.parse::<u64>() {
            return (custom, true);
        }
    }

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let concurrency_multiplier = if cores <= 2 { 1.5 } else { 1.0 };

    let profile = CommandProfile::analyze(cmd);
    match profile.workload {
        CommandWorkload::DependencyInstall => (
            ((base_secs as f64 * 4.0 * concurrency_multiplier) as u64).max(120),
            true,
        ),
        CommandWorkload::BuildOrTest => (
            ((base_secs as f64 * 3.0 * concurrency_multiplier) as u64).max(90),
            true,
        ),
        CommandWorkload::ManagedTask | CommandWorkload::ServerDaemon => (
            ((base_secs as f64 * 2.0 * concurrency_multiplier) as u64).max(60),
            true,
        ),
        CommandWorkload::InstantProbe => (base_secs, false),
    }
}

/// Determines if a shell command represents a persistent web server, dev daemon, or background process.
pub fn is_daemon_or_server_command(workspace_root: &Path, cmd: &str) -> bool {
    let profile = CommandProfile::analyze(cmd);
    if matches!(
        profile.workload,
        CommandWorkload::InstantProbe
            | CommandWorkload::DependencyInstall
            | CommandWorkload::BuildOrTest
    ) {
        return false;
    }
    if profile.workload == CommandWorkload::ServerDaemon {
        return true;
    }
    crate::dev::scripts::WorkspaceScriptDiscovery::is_server_command(workspace_root, cmd)
}

/// Spawns a persistent background dev server or daemon task under MiniDevRegistry / MiniTask Manager.
pub async fn exec_daemon_or_server(workspace_root: &Path, command_str: &str) -> Result<String> {
    let registry = crate::dev::registry::get_global_dev_registry();
    let clean_cmd = command_str.trim().trim_end_matches('&').trim();
    let profile = CommandProfile::analyze(clean_cmd);
    let port_hint = profile.port_hint;
    let name_prefix = profile.task_slug;

    let req = crate::dev::models::SpawnDevRequest {
        command: clean_cmd.to_string(),
        name: Some(name_prefix),
        process_type: crate::dev::models::DevProcessType::from_str_loose(clean_cmd),
        working_dir: None,
        extra_env: std::collections::HashMap::new(),
        port_hint,
        max_memory_mb: None,
        port_policy: Some(crate::dev::models::PortConflictPolicy::Fallback),
        restart_policy: Some(crate::dev::models::RestartPolicy::Never),
    };

    let summary = registry.spawn(workspace_root, req).await?;

    // Wait 800ms for process to initialize or report immediate fast crash
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // Inspect updated status after startup
    if let Some(updated) = registry.get(&summary.id).await {
        if let crate::dev::models::DevProcessStatus::Exited(Some(code)) = updated.status {
            if code != 0 {
                let logs = registry
                    .logs(&summary.id, 20, None)
                    .await
                    .unwrap_or_default();
                return Err(ToolError::CommandExec(format!(
                    "Background process '{}' (PID {:?}) failed to start (exit code {}):\n{}",
                    summary.name,
                    summary.pid,
                    code,
                    logs.join("\n")
                ))
                .into());
            }
        }

        let mut output = String::new();
        output.push_str(&format!(
            "Background task launched successfully and registered with MiniTask Manager:\n• Task ID: {}\n• Name: {}\n• Type: {}\n• PID: {}\n• Status: {}\n",
            updated.id.as_str(),
            updated.name,
            updated.process_type,
            updated.pid.map(|p| p.to_string()).unwrap_or_else(|| "none".to_string()),
            updated.status
        ));

        if let Some(ref url) = updated.url {
            output.push_str(&format!("• Primary URL: {}\n", url));
        } else if !updated.ports.is_empty() {
            let p = updated.ports[0];
            let host = crate::dev::ports::resolve_listening_loopback_host(p).unwrap_or("localhost");
            output.push_str(&format!(
                "• Active Ports: {:?}\n• URL: http://{}:{}\n",
                updated.ports, host, p
            ));
        } else if let Some(hint) = port_hint {
            let host =
                crate::dev::ports::resolve_listening_loopback_host(hint).unwrap_or("localhost");
            output.push_str(&format!(
                "• Expected Port: {}\n• URL: http://{}:{}\n",
                hint, host, hint
            ));
        }

        output.push_str("\nProcess is actively running and supervised under MiniTask Manager (with automatic OOM & runaway watchdog).\nUse 'minitask' or '/tasks' in TUI to inspect logs, view telemetry, or stop the task.");

        Ok(output)
    } else {
        Ok(format!(
            "🚀 Background process launched with ID: {}",
            summary.id.as_str()
        ))
    }
}

/// Structural token-stream classifier for build, compile, test, bundle, package install, and script workloads
/// across any language, runtime, or custom script. Returns a dynamically synthesized task slug (`<program>-<action>`)
/// when the command should be supervised in `minitask` (`MiniDevRegistry`).
pub fn classify_background_task_name(cmd: &str) -> Option<String> {
    let profile = CommandProfile::analyze(cmd);
    match profile.workload {
        CommandWorkload::InstantProbe => None,
        CommandWorkload::DependencyInstall
        | CommandWorkload::BuildOrTest
        | CommandWorkload::ManagedTask
        | CommandWorkload::ServerDaemon => Some(profile.task_slug),
    }
}

/// Executes a non-daemon command inside `MiniDevRegistry` (`minitask`) with Antigravity-style
/// `wait_ms_before_async` auto-backgrounding.
///
/// - If the command finishes within `wait_ms`, marks it as reported in `MiniDevRegistry` and returns
///   the full RTK-compacted + fault-localized output synchronously.
/// - If the command is still running after `wait_ms`, leaves it running in `MiniDevRegistry` (`minitask`)
///   and returns a structured background task handoff with `task_id`, live log file path, and preview logs.
pub async fn exec_managed_background_task(
    workspace_root: &Path,
    command_str: &str,
    wait_ms: u64,
    explicit_context: Option<usize>,
) -> Result<String> {
    let registry = crate::dev::registry::get_global_dev_registry();
    let clean_cmd = command_str.trim();
    let name_slug =
        classify_background_task_name(clean_cmd).unwrap_or_else(|| "bg-cmd".to_string());

    let req = crate::dev::models::SpawnDevRequest {
        command: clean_cmd.to_string(),
        name: Some(name_slug),
        process_type: crate::dev::models::DevProcessType::Script,
        working_dir: None,
        extra_env: std::collections::HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: Some(crate::dev::models::RestartPolicy::Never),
    };

    let (summary, handle) = registry
        .spawn_with_options(workspace_root, req, false)
        .await?;

    let can_detach = !crate::agent::subagent::is_inside_minipub();
    let clamped_wait_ms = wait_ms.clamp(50, 120_000);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(clamped_wait_ms);
    loop {
        let notified = handle.completion_notify.notified();
        let steering_notified = registry.notified_task_or_steering();
        if !handle.status.read().await.is_alive() || (can_detach && registry.has_steering_prompts())
        {
            break;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let remaining = deadline - now;
        tokio::select! {
            _ = notified => {}
            _ = steering_notified, if can_detach => {}
            _ = tokio::time::sleep(remaining) => break,
        }
    }

    let final_status = handle.status.read().await.clone();
    if !final_status.is_alive() {
        // Command finished within `wait_ms`! Mark as already reported so AgentLoop does not duplicate it.
        registry.mark_task_reported(&summary.id);

        let lines: Vec<String> = {
            let lock = handle.logs.read().await;
            lock.iter().cloned().collect()
        };
        let mut combined = String::new();
        let mut stderr_only = String::new();
        for line in lines {
            if let Some(rest) = line.strip_prefix("[STDERR] ") {
                if !stderr_only.is_empty() {
                    stderr_only.push('\n');
                }
                stderr_only.push_str(rest);
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str("[stderr]: ");
                combined.push_str(rest);
            } else {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&line);
            }
        }

        let exit_code = match final_status {
            crate::dev::models::DevProcessStatus::Exited(code) => code,
            crate::dev::models::DevProcessStatus::Stopped => Some(0),
            crate::dev::models::DevProcessStatus::Degraded(_) => Some(1),
            _ => Some(0),
        };

        return finalize_command_output(
            workspace_root,
            clean_cmd,
            combined,
            &stderr_only,
            exit_code,
            explicit_context,
        );
    }

    // Still running after `wait_ms` -> return Antigravity-style async background task handoff!
    let preview_lines = handle.get_logs(20, None).await;
    let log_path_display = handle
        .log_path
        .as_ref()
        .map(|p| {
            p.strip_prefix(workspace_root)
                .unwrap_or(p.as_path())
                .display()
                .to_string()
        })
        .unwrap_or_else(|| format!(".minicode/logs/tasks/{}.log", summary.id.as_str()));

    let mut out = format!(
        "⏳ Command still running after {}ms — running asynchronously in `minitask` (`MiniDevRegistry`).\n\
         • Task ID: `{}`\n\
         • Name: `{}`\n\
         • PID: `{}`\n\
         • Status: `Running`\n\
         • Command: `{}`\n\
         • Live Log File: `{}`\n",
        clamped_wait_ms,
        summary.id.as_str(),
        summary.name,
        handle.pid(),
        clean_cmd,
        log_path_display
    );

    if !preview_lines.is_empty() {
        out.push_str("• Initial Output:\n```\n");
        out.push_str(&preview_lines.join("\n"));
        out.push_str("\n```\n");
    }

    out.push_str(&format!(
        "\n🔔 **Reactive Notification:** The system will automatically notify you with `<completed_background_tasks>` as soon as `{id}` finishes — do NOT poll in a loop.\n\
         🛠️ **Task Controls:** You may continue parallel work now, or call `minitask(action=\"await\", id=\"{id}\")`, `minitask(action=\"logs\", id=\"{id}\")`, `minitask(action=\"send_input\", id=\"{id}\", input=\"...\")`, or `minitask(action=\"kill\", id=\"{id}\")`.",
        id = summary.id.as_str()
    ));
    if can_detach && registry.has_steering_prompts() {
        out.push_str(
            "\n⚡ **Live User Prompt Received:** The user sent a live prompt while this task continues running in `minitask`. Handle the user's live prompt immediately; `minitask` will automatically notify you when this background task finishes.",
        );
    }

    Ok(out)
}

/// Executes a shell command inside the sandboxed workspace environment using active context window limit.
pub async fn exec_cmd(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
) -> Result<String> {
    exec_cmd_with_context(workspace_root, command_str, timeout_secs, None).await
}

/// Executes a shell command with an optional explicit context window limit.
pub async fn exec_cmd_with_context(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
    explicit_context: Option<usize>,
) -> Result<String> {
    exec_cmd_full(
        workspace_root,
        command_str,
        timeout_secs,
        explicit_context,
        false,
    )
    .await
}

/// Executes a shell command with Antigravity-style `wait_ms_before_async` and `run_in_background` support.
pub async fn exec_cmd_with_async_options(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
    explicit_context: Option<usize>,
    is_daemon: bool,
    wait_ms_before_async: Option<u64>,
    run_in_background: Option<bool>,
) -> Result<String> {
    if is_daemon || is_daemon_or_server_command(workspace_root, command_str) {
        return exec_daemon_or_server(workspace_root, command_str).await;
    }

    if run_in_background == Some(true) {
        let wait_ms = wait_ms_before_async.unwrap_or(500);
        return exec_managed_background_task(
            workspace_root,
            command_str,
            wait_ms,
            explicit_context,
        )
        .await;
    }

    if let Some(wait_ms) = wait_ms_before_async {
        return exec_managed_background_task(
            workspace_root,
            command_str,
            wait_ms,
            explicit_context,
        )
        .await;
    }

    if classify_background_task_name(command_str).is_some() {
        let max_unbounded_wait = if crate::agent::subagent::is_inside_minipub() {
            10_000
        } else {
            2_500
        };
        let default_wait_ms = match timeout_secs {
            Some(s) => s.saturating_mul(1000).clamp(500, 120_000),
            None => max_unbounded_wait,
        };
        return exec_managed_background_task(
            workspace_root,
            command_str,
            default_wait_ms,
            explicit_context,
        )
        .await;
    }

    exec_cmd_full(
        workspace_root,
        command_str,
        timeout_secs,
        explicit_context,
        false,
    )
    .await
}

/// Executes a shell command with full control over context limit and background daemon execution.
pub async fn exec_cmd_full(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
    explicit_context: Option<usize>,
    is_daemon: bool,
) -> Result<String> {
    if is_daemon || is_daemon_or_server_command(workspace_root, command_str) {
        return exec_daemon_or_server(workspace_root, command_str).await;
    }

    let timeout = resolve_smart_exec_timeout(command_str, timeout_secs);

    let mut std_cmd = build_sanitized_command("sh", workspace_root);
    std_cmd.arg("-c").arg(command_str);

    #[cfg(target_os = "linux")]
    {
        let ws = workspace_root.to_path_buf();
        unsafe {
            use std::os::unix::process::CommandExt;
            std_cmd.pre_exec(move || {
                apply_landlock_sandbox(&ws, true).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("Landlock sandbox failed: {}", e),
                    )
                })
            });
        }
    }

    let mut tokio_cmd = tokio::process::Command::from(std_cmd);
    tokio_cmd.kill_on_drop(true);
    tokio_cmd.stdin(std::process::Stdio::null());
    tokio_cmd.stdout(std::process::Stdio::piped());
    tokio_cmd.stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    {
        tokio_cmd.process_group(0);
    }

    let mut child = tokio_cmd
        .spawn()
        .map_err(|e| ToolError::CommandExec(format!("Process spawn error: {}", e)))?;

    let child_pid = child.id().unwrap_or(0);
    if child_pid > 0 {
        crate::dev::registry::get_global_dev_registry().register_external_pid(child_pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let max_read_bytes = (crate::constants::EXEC_MAX_OUTPUT_BYTES as u64) + 1;

    let read_stdout = async {
        if let Some(mut out) = stdout {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut out).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stdout_buf).await;
            // Drain any remaining output to sink so child process does not block on full OS pipe buffer
            let _ = tokio::io::copy(&mut out, &mut tokio::io::sink()).await;
        }
    };

    let read_stderr = async {
        if let Some(mut err) = stderr {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut err).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stderr_buf).await;
            // Drain any remaining output to sink so child process does not block on full OS pipe buffer
            let _ = tokio::io::copy(&mut err, &mut tokio::io::sink()).await;
        }
    };

    let run_fut = async {
        tokio::join!(read_stdout, read_stderr);
        child.wait().await
    };

    let status = match tokio::time::timeout(timeout, run_fut).await {
        Ok(Ok(s)) => {
            #[cfg(unix)]
            if child_pid > 0 {
                let p_i32 = child_pid as i32;
                let descendants = crate::dev::ports::find_all_descendants(child_pid);
                unsafe {
                    let _ = libc::kill(-p_i32, libc::SIGTERM);
                    for &d in &descendants {
                        let _ = libc::kill(d as i32, libc::SIGTERM);
                        let _ = libc::kill(-(d as i32), libc::SIGTERM);
                    }
                }
                crate::dev::registry::get_global_dev_registry().unregister_external_pid(child_pid);
            }
            s
        }
        Ok(Err(e)) => {
            #[cfg(unix)]
            if child_pid > 0 {
                let p_i32 = child_pid as i32;
                unsafe {
                    let _ = libc::kill(-p_i32, libc::SIGTERM);
                }
                crate::dev::registry::get_global_dev_registry().unregister_external_pid(child_pid);
            }
            return Err(ToolError::CommandExec(format!("Process execution error: {}", e)).into());
        }
        Err(_) => {
            #[cfg(unix)]
            if child_pid > 0 {
                let p_i32 = child_pid as i32;
                let descendants = crate::dev::ports::find_all_descendants(child_pid);
                unsafe {
                    let _ = libc::kill(-p_i32, libc::SIGTERM);
                    let _ = libc::kill(p_i32, libc::SIGTERM);
                    for &d in &descendants {
                        let _ = libc::kill(d as i32, libc::SIGTERM);
                        let _ = libc::kill(-(d as i32), libc::SIGTERM);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(
                    crate::constants::PROCESS_KILL_GRACE_PERIOD_MS,
                ))
                .await;
                unsafe {
                    let _ = libc::kill(-p_i32, libc::SIGKILL);
                    let _ = libc::kill(p_i32, libc::SIGKILL);
                    for &d in &descendants {
                        let _ = libc::kill(d as i32, libc::SIGKILL);
                        let _ = libc::kill(-(d as i32), libc::SIGKILL);
                    }
                }
                crate::dev::registry::get_global_dev_registry().unregister_external_pid(child_pid);
            }
            let _ = child.kill().await;
            return Err(ToolError::CommandTimeout {
                timeout_secs: timeout.as_secs(),
            }
            .into());
        }
    };

    let stdout_str = String::from_utf8_lossy(&stdout_buf);
    let stderr_str = String::from_utf8_lossy(&stderr_buf);

    let mut combined = String::new();
    if !stdout_str.is_empty() {
        combined.push_str(&stdout_str);
    }
    if !stderr_str.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str("[stderr]: ");
        combined.push_str(&stderr_str);
    }

    let exit_code = status.code();
    finalize_command_output(
        workspace_root,
        command_str,
        combined,
        &stderr_str,
        exit_code,
        explicit_context,
    )
}

fn finalize_command_output(
    workspace_root: &Path,
    command_str: &str,
    mut combined: String,
    stderr_str: &str,
    exit_code: Option<i32>,
    explicit_context: Option<usize>,
) -> Result<String> {
    if combined.len() > crate::constants::EXEC_MAX_OUTPUT_BYTES {
        let valid_end = combined.floor_char_boundary(crate::constants::EXEC_MAX_OUTPUT_BYTES);
        let truncated = &combined[..valid_end];
        combined = format!(
            "{}\n\n[... Output truncated: exceeded max limit ...]",
            truncated
        );
    }

    let status_code = exit_code.unwrap_or(crate::constants::SIGNAL_KILLED_EXIT_CODE);

    // Preserve full execution output on disk if output exceeds threshold (Phase 115, Phase 117)
    let context_window = explicit_context.unwrap_or_else(|| resolve_context_window(workspace_root));
    let line_threshold = if context_window >= crate::constants::CONTEXT_WINDOW_128K {
        crate::constants::DONUT_EXTENDED_THRESHOLD_LINES
    } else {
        crate::constants::DONUT_STANDARD_THRESHOLD_LINES
    };

    let total_lines = combined.lines().count();
    let total_bytes = combined.len();
    let is_truncated =
        total_lines > line_threshold || total_bytes > crate::constants::EXEC_MAX_OUTPUT_BYTES;

    let rtk_res = super::rtk_filter::RtkFilter::filter_for_context(
        command_str,
        &combined,
        exit_code,
        context_window,
    );
    let mut compacted = super::compactor::compact_tool_output_for_context(
        command_str,
        &rtk_res.content,
        exit_code,
        context_window,
    );

    let compacted_lines = compacted.lines().count();
    let significantly_compacted = (total_lines > 40 || total_bytes > 4096)
        && (compacted_lines * 100 < total_lines.max(1) * 65
            || compacted.len() * 100 < total_bytes.max(1) * 65);

    if is_truncated || significantly_compacted {
        let logs_dir = workspace_root.join(".minicode").join("logs");
        if let Err(e) = std::fs::create_dir_all(&logs_dir) {
            tracing::debug!("Could not create .minicode/logs: {}", e);
        }
        let log_file = logs_dir.join("last_exec.log");
        let redacted_raw = crate::sandbox::redact::SecretRedactor::global().redact(&combined);
        if let Err(e) = std::fs::write(&log_file, &redacted_raw) {
            tracing::debug!("Could not write last_exec.log: {}", e);
        }
        let spillover_path =
            super::middleware::save_spillover_output(workspace_root, "exec_cmd", &combined)
                .unwrap_or_else(|| ".minicode/logs/last_exec.log".to_string());
        let log_notice = format!(
            "\n[Full untruncated output ({} lines, {}B) saved to {} (and .minicode/logs/last_exec.log) — use read_file or grep_search on this path if you need omitted lines]",
            total_lines, total_bytes, spillover_path
        );
        if !compacted.contains("last_exec.log") && !compacted.contains("tmp/tool_outputs/") {
            compacted.push_str(&log_notice);
        }
    }
    compacted = crate::security::sanitize_text(&compacted);

    if status_code != 0 {
        let trailing_cmd = command_str
            .rsplit([';', '&', '|'])
            .next()
            .unwrap_or(command_str)
            .trim();
        let trailing_prog = trailing_cmd.split_whitespace().next().unwrap_or("");
        if status_code == 1
            && stderr_str.trim().is_empty()
            && matches!(trailing_prog, "grep" | "egrep" | "fgrep" | "rg")
        {
            return if compacted.trim().is_empty() {
                Ok("(0 matches found)".to_string())
            } else {
                Ok(format!("{}\n(0 matches found)", compacted.trim_end()))
            };
        }

        let frames = crate::context::search::fault_localizer::FaultLocalizer::extract_trace_frames(
            &combined,
        );
        let mut fault_hint = String::new();
        if !frames.is_empty() {
            fault_hint.push_str("\n\n[Where] 📍 Probable Fault Sites:\n");
            let mut seen_locations = std::collections::HashSet::new();
            for frame in frames.iter().take(4) {
                let key = (frame.file_path.clone(), frame.line_number);
                if seen_locations.insert(key) {
                    let start_suggest = frame.line_number.saturating_sub(10).max(1);
                    let end_suggest = frame.line_number + 15;
                    fault_hint.push_str(&format!(
                        "  • `{}:{}` -> Suggested action: `read_file(path: \"{}\", start_line: {}, end_line: {})`\n",
                        frame.file_path, frame.line_number, frame.file_path, start_suggest, end_suggest
                    ));
                }
            }
            fault_hint.push_str("\n[Suggested Next Action]\nInspect the failing code envelopes at the fault sites above and apply surgical repairs using `patch_file`.");
        }

        let out_body = if compacted.trim().is_empty() {
            "(No output captured on stdout/stderr)".to_string()
        } else {
            compacted
        };

        return Err(ToolError::CommandExec(format!(
            "Command exited with non-zero status ({status_code}):\n{}{}",
            out_body, fault_hint
        ))
        .into());
    }

    if compacted.trim().is_empty() {
        Ok("Command executed successfully (no output).".to_string())
    } else {
        Ok(compacted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exec_echo() {
        let temp_dir = std::env::temp_dir();
        let out = exec_cmd(&temp_dir, "echo 'hello minicode'", Some(5))
            .await
            .unwrap();
        assert!(out.contains("hello minicode"));
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let temp_dir = std::env::temp_dir();
        let res = exec_cmd(&temp_dir, "sleep 3", Some(1)).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_exec_nonzero_exit_returns_err_with_fault_sites() {
        let temp_dir = std::env::temp_dir();
        let out = exec_cmd(
            &temp_dir,
            "echo 'error[E0425]: cannot find value `foo` in this scope\n  --> src/main.rs:42:15'; exit 1",
            Some(5),
        )
        .await;
        assert!(out.is_err());
        let err_msg = out.unwrap_err().to_string();
        assert!(err_msg.contains("non-zero status (1)"));
        assert!(err_msg.contains("Probable Fault Sites"));
        assert!(err_msg.contains("src/main.rs:42"));
    }

    #[tokio::test]
    async fn test_exec_grep_zero_matches_succeeds_cleanly() {
        let temp_dir = std::env::temp_dir();
        let out = exec_cmd(
            &temp_dir,
            "echo 'checked'; printf 'alpha\\nbeta\\n' | grep 'nonexistent_token_xyz'",
            Some(5),
        )
        .await
        .expect("grep with 0 matches and empty stderr should succeed");
        assert!(out.contains("checked"));
        assert!(out.contains("(0 matches found)"));
    }

    #[test]
    fn test_resolve_smart_exec_timeout() {
        // Explicit timeout for lightweight commands is honored
        assert_eq!(
            resolve_smart_exec_timeout("ls -la", Some(10)),
            Duration::from_secs(10)
        );

        // Package managers dynamically scale up even if caller passed schema default 30s
        assert!(resolve_smart_exec_timeout("npm install lucide-react", Some(30)).as_secs() >= 120);

        // Heavy install operations dynamically scale to at least 120s
        assert!(resolve_smart_exec_timeout("npm install lucide-react", None).as_secs() >= 120);
        assert!(resolve_smart_exec_timeout("pnpm add react", None).as_secs() >= 60);
        assert!(resolve_smart_exec_timeout("cargo test -j 1", None).as_secs() >= 90);
        assert!(resolve_smart_exec_timeout("uv sync", None).as_secs() >= 120);

        // Explicit large timeout is honored
        assert_eq!(
            resolve_smart_exec_timeout("npm install lucide-react", Some(300)),
            Duration::from_secs(300)
        );

        // Regular commands stay at default 30s (or dynamic base)
        assert_eq!(
            resolve_smart_exec_timeout("ls -la", None),
            Duration::from_secs(30)
        );
        assert_eq!(
            resolve_smart_exec_timeout("cat src/main.rs", None),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn test_classify_background_task_name() {
        assert_eq!(
            classify_background_task_name("bun build src/index.ts --outdir dist").as_deref(),
            Some("bun-build")
        );
        assert_eq!(
            classify_background_task_name("CARGO_BUILD_JOBS=1 cargo test --lib").as_deref(),
            Some("cargo-test")
        );
        assert_eq!(
            classify_background_task_name("pytest -q tests/").as_deref(),
            Some("pytest-tests")
        );
        assert_eq!(
            classify_background_task_name("go build ./...").as_deref(),
            Some("go-build")
        );
        assert_eq!(
            classify_background_task_name("tsc --noEmit").as_deref(),
            Some("tsc")
        );
        assert_eq!(
            classify_background_task_name("bash localupdate.sh").as_deref(),
            Some("bash-localupdate")
        );
        assert_eq!(classify_background_task_name("cargo --version"), None);
        assert_eq!(classify_background_task_name("ls -la"), None);
        assert_eq!(
            classify_background_task_name(
                "cd /home/aswin/programming/vscode/minitest/minitest41 && grep -rn \"foo\" src/"
            ),
            None
        );
        assert_eq!(
            classify_background_task_name(
                "ls node_modules/.bin > /tmp/bins.txt 2>&1; cat /tmp/bins.txt"
            ),
            None
        );
        assert_eq!(
            classify_background_task_name(
                "cd /home/aswin/programming/vscode/minitest/minitest41 && bun run build"
            )
            .as_deref(),
            Some("bun-build")
        );
    }

    #[tokio::test]
    async fn test_exec_cmd_wait_ms_before_async_promotion_and_completion() {
        let temp = tempfile::tempdir().expect("tempdir");

        // 1. Fast command finishes within wait_ms_before_async (2000ms) -> returns output directly
        let fast_out = exec_cmd_with_async_options(
            temp.path(),
            "echo 'fast-build-ok'",
            Some(5),
            None,
            false,
            Some(2000),
            None,
        )
        .await
        .expect("fast command succeeds");
        assert!(fast_out.contains("fast-build-ok"));

        // 2. Slow command exceeds wait_ms_before_async (150ms) -> promotes to async task in minitask
        let async_out = exec_cmd_with_async_options(
            temp.path(),
            "echo 'starting-bundle'; sleep 1; echo 'bundle-complete'",
            Some(10),
            None,
            false,
            Some(150),
            None,
        )
        .await
        .expect("async handoff succeeds");
        assert!(
            async_out.contains("running asynchronously in `minitask`"),
            "Expected minitask async handoff, got:\n{}",
            async_out
        );
        assert!(async_out.contains(".minicode/logs/tasks/"));

        // Wait for background task to finish and verify reactive harvest
        let registry = crate::dev::registry::get_global_dev_registry();
        registry
            .wait_until_active_tasks_settle(Duration::from_secs(4))
            .await;
        let (_ids, block) = registry
            .harvest_newly_completed_tasks_block(temp.path())
            .await
            .expect("expected harvested completed task");
        assert!(block.contains("<completed_background_tasks>"));
        assert!(block.contains("bundle-complete"));
    }

    #[test]
    fn test_minitest50_detached_subshell_and_perl_eval_workload_classification() {
        // 1. Compound command containing a detached background subshell `(bun run preview ... &)`
        //    followed by synchronous `sleep` + `curl` MUST classify as InstantProbe, never ServerDaemon!
        let compound = "pkill -f \"vite preview\"; sleep 1; (bun run preview -- --port 4181 >/tmp/preview.log 2>&1 &); sleep 5; curl http://127.0.0.1:4180";
        let prof = CommandProfile::analyze(compound);
        assert_eq!(prof.workload, CommandWorkload::InstantProbe);
        assert!(!is_daemon_or_server_command(Path::new("."), compound));
        assert_eq!(classify_background_task_name(compound), None);

        // 2. Inline perl / python / node one-liner evaluations MUST classify as InstantProbe!
        let perl_cmd = "perl -0pi -e 's/foo/bar/g' src/components/sections/DualMode.tsx";
        assert_eq!(
            CommandProfile::analyze(perl_cmd).workload,
            CommandWorkload::InstantProbe
        );
        assert_eq!(classify_background_task_name(perl_cmd), None);
        assert_eq!(
            CommandProfile::analyze("python3 -c \"print(42)\"").workload,
            CommandWorkload::InstantProbe
        );
        assert_eq!(
            CommandProfile::analyze("node -e \"console.log(1)\"").workload,
            CommandWorkload::InstantProbe
        );

        // 3. True standalone servers (including `-p <port>`) still classify as ServerDaemon!
        assert_eq!(
            CommandProfile::analyze("bun run dev -- --port 5180").workload,
            CommandWorkload::ServerDaemon
        );
        assert_eq!(
            CommandProfile::analyze("http-server -p 8080").workload,
            CommandWorkload::ServerDaemon
        );
    }

    #[tokio::test]
    async fn test_exec_cmd_honors_explicit_timeout_secs_without_2500ms_clamp() {
        let temp = tempfile::tempdir().expect("tempdir");
        // Command sleeps 3s (> old 2.5s clamp) with explicit timeout_secs = Some(8) and wait_ms_before_async = None;
        // must execute synchronously to completion without backgrounding into minitask!
        let out = exec_cmd_with_async_options(
            temp.path(),
            "sleep 3; echo 'sync-after-3s-ok'",
            Some(8),
            None,
            false,
            None,
            None,
        )
        .await
        .expect("explicit timeout command succeeds synchronously");
        assert!(
            out.contains("sync-after-3s-ok"),
            "Expected synchronous completion output, got:\n{}",
            out
        );
        assert!(!out.contains("running asynchronously in `minitask`"));
    }

    #[test]
    fn test_minitest51_heredoc_python_inline_eval_and_segment_splitting() {
        let heredoc_cmd = r#"python3 - << 'EOF'
with open('FeatureTabs.tsx', 'w') as f:
    f.write("""export function FeatureTabs() { return <div />; };""")
print("patched; done && verified")
EOF
grep -c "FeatureTabs" FeatureTabs.tsx"#;

        let segs = CommandProfile::split_shell_segments(heredoc_cmd);
        assert_eq!(
            segs.len(),
            2,
            "Heredoc body must stay intact inside segment 0; got: {:?}",
            segs
        );
        assert!(segs[0].0.starts_with("python3 - << 'EOF'"));
        assert!(segs[0].0.ends_with("EOF"));
        assert_eq!(segs[1].0, "grep -c \"FeatureTabs\" FeatureTabs.tsx");

        let prof = CommandProfile::analyze(heredoc_cmd);
        assert_eq!(prof.workload, CommandWorkload::InstantProbe);
        assert_eq!(classify_background_task_name(heredoc_cmd), None);
    }

    #[test]
    fn test_minitest54_posix_for_loop_probe_not_misclassified_as_managed_task() {
        let loop_probe = r#"cd /tmp && for f in src/App.tsx src/pages/HomePage.tsx src/components/Navbar.tsx; do echo "=== $f ==="; cat -n "$f"; done"#;
        let prof = CommandProfile::analyze(loop_probe);
        assert_eq!(prof.workload, CommandWorkload::InstantProbe);
        assert_eq!(classify_background_task_name(loop_probe), None);

        let loop_build = r#"for pkg in core cli; do cargo test -p "$pkg"; done"#;
        let build_prof = CommandProfile::analyze(loop_build);
        assert_eq!(build_prof.workload, CommandWorkload::BuildOrTest);
    }
}
