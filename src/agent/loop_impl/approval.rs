//! Mandatory human-approval classification for destructive or remote-mutating tool and shell invocations.

use std::path::{Component, Path, PathBuf};

/// Classifies whether a tool invocation is a critical remote-mutating or destructive operation
/// (`git push`, `git pull`, `git reset --hard`, `git clean -f`, `gh pr merge/create`, `cargo/npm publish`,
/// unconfined `rm -rf`, `sudo`, etc.) that MUST ALWAYS ask for explicit user permission in interactive mode,
/// even when `auto_approve` ("Allow session") is enabled.
pub fn classify_mandatory_approval(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    classify_mandatory_approval_with_workspace(tool_name, args, None)
}

/// Workspace-aware variant of [`classify_mandatory_approval`] that validates path confinement
/// against `workspace_root` (e.g., permitting nested starter cleanup like `rm -rf src/providers`
/// strictly inside `workspace_root` while still prompting on whole-tree `rm -rf src`, `.git`,
/// `.minicode`, `.env`, globs, `..`, or any path outside `workspace_root`).
pub fn classify_mandatory_approval_with_workspace(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: Option<&Path>,
) -> Option<String> {
    match tool_name {
        "git_push" => Some("Remote Git push requires explicit user permission".to_string()),
        "git_pull" => Some("Remote Git pull requires explicit user permission".to_string()),
        "create_pr" | "github_pr_create" => Some(
            "Creating a remote GitHub Pull Request requires explicit user permission".to_string(),
        ),
        "github_issue_create" => {
            Some("Creating a remote GitHub Issue requires explicit user permission".to_string())
        }
        "exec_cmd" | "sandbox_exec" => {
            let parsed_str_args: Option<serde_json::Value> =
                args.as_str().and_then(|s| serde_json::from_str(s).ok());
            let effective_args = parsed_str_args.as_ref().unwrap_or(args);
            let raw_cmd = effective_args
                .get("command")
                .or_else(|| effective_args.get("cmd"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if raw_cmd.trim().is_empty() {
                return None;
            }
            let effective_ws: Option<PathBuf> = match workspace_root {
                Some(ws) => {
                    if let Some(cwd_str) = effective_args
                        .get("cwd")
                        .or_else(|| effective_args.get("working_dir"))
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty() && *s != ".")
                    {
                        resolve_confined_workdir(ws, Path::new(cwd_str))
                    } else {
                        Some(ws.to_path_buf())
                    }
                }
                None => None,
            };
            classify_mandatory_shell_command_with_workspace(raw_cmd, effective_ws.as_deref())
        }
        _ => None,
    }
}

/// Inspects a shell command string for critical operations (`git push`, `git pull`, `git reset --hard`,
/// `git clean -f`, `gh pr merge`, `cargo/npm/bun publish`, unconfined `rm -rf`, `sudo`, etc.) that must always prompt.
pub fn classify_mandatory_shell_command(command_str: &str) -> Option<String> {
    classify_mandatory_shell_command_with_workspace(command_str, None)
}

/// Workspace-aware shell command classifier.
pub fn classify_mandatory_shell_command_with_workspace(
    command_str: &str,
    workspace_root: Option<&Path>,
) -> Option<String> {
    let mut current_cwd: Option<PathBuf> = workspace_root.map(Path::to_path_buf);

    for segment in command_str.split([';', '\n']) {
        for sub in segment.split("&&").flat_map(|s| s.split("||")) {
            let tokens: Vec<&str> = sub.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            // Skip leading env assignments (KEY=VAL) and wrapper commands (timeout, nice, env)
            let mut idx = 0;
            while idx < tokens.len() {
                let tok = tokens[idx];
                if tok.contains('=') && !tok.starts_with('-') {
                    idx += 1;
                    continue;
                }
                if matches!(tok, "env" | "command" | "nohup") {
                    idx += 1;
                    continue;
                }
                if tok == "timeout" {
                    idx += 1;
                    while idx < tokens.len() && tokens[idx].starts_with('-') {
                        idx += 1;
                    }
                    if idx < tokens.len() {
                        idx += 1; // duration arg
                    }
                    continue;
                }
                if tok == "nice" {
                    idx += 1;
                    if idx < tokens.len() && tokens[idx] == "-n" {
                        idx += 2;
                    }
                    continue;
                }
                break;
            }

            let rem = &tokens[idx..];
            if rem.is_empty() {
                continue;
            }

            let bin = rem[0]
                .rsplit('/')
                .next()
                .unwrap_or(rem[0])
                .to_ascii_lowercase();

            if bin == "cd" {
                current_cwd = match (workspace_root, current_cwd.as_deref(), rem.get(1)) {
                    (Some(root), Some(base), Some(&dest)) if !dest.is_empty() && dest != "-" => {
                        let candidate = if Path::new(dest).is_absolute() {
                            PathBuf::from(dest)
                        } else {
                            base.join(dest)
                        };
                        resolve_confined_workdir(root, &candidate)
                    }
                    _ => None,
                };
                continue;
            }

            if bin == "sudo" || bin == "doas" {
                return Some(format!(
                    "Privilege escalation (`{}`) requires explicit user permission",
                    bin
                ));
            }

            if bin == "git" {
                // Find the git subcommand (skipping global flags like `-C <path>`, `--no-pager`, `-c key=val`)
                let mut g_idx = 1;
                while g_idx < rem.len() {
                    let g_tok = rem[g_idx];
                    if matches!(g_tok, "-C" | "-c" | "--git-dir" | "--work-tree") {
                        g_idx += 2;
                        continue;
                    }
                    if g_tok.starts_with('-') {
                        g_idx += 1;
                        continue;
                    }
                    break;
                }
                if let Some(&subcmd_raw) = rem.get(g_idx) {
                    let subcmd = subcmd_raw.to_ascii_lowercase();
                    let sub_args = &rem[g_idx + 1..];
                    match subcmd.as_str() {
                        "push" => {
                            return Some(
                                "`git push` (pushing commits to remote repository) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "pull" => {
                            return Some(
                                "`git pull` (pulling remote changes into workspace) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "reset" if sub_args.iter().any(|a| *a == "--hard" || *a == "--merge") => {
                            return Some(
                                "`git reset --hard` (discarding uncommitted work) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "clean"
                            if sub_args.iter().any(|a| {
                                a.starts_with('-') && (a.contains('f') || *a == "--force")
                            }) =>
                        {
                            return Some(
                                "`git clean -f` (deleting untracked files) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "checkout"
                            if sub_args.contains(&".")
                                || sub_args.windows(2).any(|w| w == ["--", "."]) =>
                        {
                            return Some(
                                "`git checkout -- .` (discarding working tree edits) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "restore" if sub_args.contains(&".") => {
                            return Some(
                                "`git restore .` (discarding working tree edits) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        "branch" if sub_args.contains(&"-D") => {
                            return Some(
                                "`git branch -D` (force-deleting a branch) requires explicit user permission"
                                    .to_string(),
                            );
                        }
                        _ => {}
                    }
                }
            }

            if bin == "gh" && rem.len() >= 3 {
                let domain = rem[1].to_ascii_lowercase();
                let action = rem[2].to_ascii_lowercase();
                if matches!(
                    (domain.as_str(), action.as_str()),
                    ("pr", "create" | "merge" | "close")
                        | ("release", "create" | "delete")
                        | ("repo", "delete" | "archive" | "create")
                        | ("issue", "create" | "delete" | "close")
                ) {
                    return Some(format!(
                        "`gh {} {}` (remote GitHub operation) requires explicit user permission",
                        domain, action
                    ));
                }
            }

            if matches!(bin.as_str(), "cargo" | "npm" | "bun" | "pnpm" | "yarn")
                && rem
                    .get(1)
                    .is_some_and(|a| a.eq_ignore_ascii_case("publish"))
            {
                return Some(format!(
                    "`{} publish` (publishing package to remote registry) requires explicit user permission",
                    bin
                ));
            }

            if bin == "docker" && rem.get(1).is_some_and(|a| a.eq_ignore_ascii_case("push")) {
                return Some("`docker push` requires explicit user permission".to_string());
            }

            if bin == "rm" {
                let has_recursive = rem[1..].iter().any(|a| {
                    *a == "--recursive"
                        || (a.starts_with('-')
                            && !a.starts_with("--")
                            && (a.contains('r') || a.contains('R')))
                });
                if has_recursive {
                    let targets: Vec<&str> = rem[1..]
                        .iter()
                        .copied()
                        .filter(|a| !a.starts_with('-'))
                        .collect();
                    let all_safe = !targets.is_empty()
                        && targets.iter().all(|t| {
                            is_safe_recursive_rm_target(t, workspace_root, current_cwd.as_deref())
                        });
                    if !all_safe {
                        return Some(
                            "Recursive deletion (`rm -rf`) requires explicit user permission"
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
    None
}

fn has_shell_expansion_or_glob(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c,
            '*' | '?'
                | '['
                | ']'
                | '{'
                | '}'
                | '$'
                | '~'
                | '`'
                | '\\'
                | '\''
                | '"'
                | '|'
                | '&'
                | '<'
                | '>'
        )
    })
}

/// Lexically normalizes a candidate directory relative to `workspace_root` and verifies that it
/// stays at or inside `workspace_root` (rejecting `..` escapes, shell metacharacters, and symlink escapes).
fn resolve_confined_workdir(workspace_root: &Path, candidate: &Path) -> Option<PathBuf> {
    let cand_str = candidate.to_str()?;
    if cand_str.is_empty() || has_shell_expansion_or_glob(cand_str) {
        return None;
    }
    let rel = if candidate.is_absolute() {
        candidate.strip_prefix(workspace_root).ok()?
    } else {
        candidate
    };
    let mut normalized = workspace_root.to_path_buf();
    let mut depth = 0usize;
    for comp in rel.components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(seg) => {
                normalized.push(seg);
                depth += 1;
            }
            Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                normalized.pop();
                depth -= 1;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    verify_symlink_confinement(workspace_root, &normalized)?;
    Some(normalized)
}

/// Verifies that the deepest existing ancestor of `target` (and `target` itself, if it exists)
/// canonicalizes strictly inside (or equal to, for workdir) `workspace_root`.
fn verify_symlink_confinement(workspace_root: &Path, target: &Path) -> Option<PathBuf> {
    let canon_ws =
        std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let mut probe = target;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let canon_probe = std::fs::canonicalize(probe).ok()?;
    if !canon_probe.starts_with(&canon_ws) {
        return None;
    }
    Some(canon_ws)
}

/// Algorithmically verifies whether an `rm -r` operand is safe to execute autonomously:
/// 1. Rejects globs, shell expansions, quotes, escapes, `..` traversal, `.` / workspace root,
///    and protected repository/credential/runtime paths (`.git`, `.minicode`, `.github`, `.env*`, etc.).
/// 2. Allows standard ephemeral build/cache directories (`dist`, `node_modules`, `target`, `.vite`, etc.).
/// 3. When `workspace_root` and `current_cwd` are verified, allows nested workspace subpaths
///    (depth >= 2 relative to `workspace_root`, e.g., `src/components/ui` or `src/providers`) while
///    still prompting before deleting an entire top-level source tree (`rm -rf src`).
fn is_safe_recursive_rm_target(
    raw_target: &str,
    workspace_root: Option<&Path>,
    current_cwd: Option<&Path>,
) -> bool {
    let trimmed = raw_target.trim();
    if trimmed.is_empty() || has_shell_expansion_or_glob(trimmed) {
        return false;
    }
    if workspace_root.is_some() && current_cwd.is_none() {
        return false;
    }

    let raw_path = Path::new(trimmed);
    // Construct path relative to `workspace_root` (when available) or local relative segments
    let rel_from_ws: PathBuf = match (workspace_root, current_cwd) {
        (Some(ws), Some(cwd)) => {
            let joined = if raw_path.is_absolute() {
                raw_path.to_path_buf()
            } else {
                cwd.join(raw_path)
            };
            let Ok(stripped) = joined.strip_prefix(ws) else {
                return false;
            };
            stripped.to_path_buf()
        }
        _ => {
            if raw_path.is_absolute() {
                return false;
            }
            raw_path.to_path_buf()
        }
    };

    let mut segs: Vec<&str> = Vec::new();
    for comp in rel_from_ws.components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(os) => {
                let Some(s) = os.to_str() else {
                    return false;
                };
                if s.is_empty() {
                    return false;
                }
                segs.push(s);
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return false;
            }
        }
    }

    if segs.is_empty() {
        return false;
    }

    // Never auto-approve recursive deletion touching repository metadata, agent state, or secrets
    let touches_protected = segs.iter().any(|seg| {
        let lower = seg.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            ".git" | ".minicode" | ".github" | ".husky" | ".ssh" | ".gnupg"
        ) || lower == ".env"
            || lower.starts_with(".env.")
    });
    if touches_protected {
        return false;
    }

    let is_ephemeral_cache_dir = matches!(
        segs[0],
        "dist"
            | "node_modules"
            | "target"
            | ".next"
            | ".nuxt"
            | ".svelte-kit"
            | "build"
            | "out"
            | "coverage"
            | ".turbo"
            | ".vite"
            | ".cache"
            | ".parcel-cache"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
    );

    if !is_ephemeral_cache_dir
        && (workspace_root.is_none() || current_cwd.is_none() || segs.len() < 2)
    {
        return false;
    }

    if let Some(ws) = workspace_root {
        let resolved_full = ws.join(&rel_from_ws);
        let Some(canon_ws) = verify_symlink_confinement(ws, &resolved_full) else {
            return false;
        };
        if resolved_full.exists() {
            if let Ok(canon_target) = std::fs::canonicalize(&resolved_full) {
                if canon_target == canon_ws || !canon_target.starts_with(&canon_ws) {
                    return false;
                }
            }
        }
    }

    true
}
