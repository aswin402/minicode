//! Mandatory human-approval classification for destructive or remote-mutating tool and shell invocations.

/// Classifies whether a tool invocation is a critical remote-mutating or destructive operation
/// (`git push`, `git pull`, `git reset --hard`, `git clean -f`, `gh pr merge/create`, `cargo/npm publish`,
/// `rm -rf`, `sudo`, etc.) that MUST ALWAYS ask for explicit user permission in interactive mode,
/// even when `auto_approve` ("Allow session") is enabled.
pub fn classify_mandatory_approval(tool_name: &str, args: &serde_json::Value) -> Option<String> {
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
            classify_mandatory_shell_command(raw_cmd)
        }
        _ => None,
    }
}

/// Inspects a shell command string for critical operations (`git push`, `git pull`, `git reset --hard`,
/// `git clean -f`, `gh pr merge`, `cargo/npm/bun publish`, `rm -rf`, `sudo`, etc.) that must always prompt.
pub fn classify_mandatory_shell_command(command_str: &str) -> Option<String> {
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
                let has_rf = rem[1..].iter().any(|a| {
                    *a == "-rf" || *a == "-fr" || *a == "-r" || *a == "-R" || *a == "--recursive"
                });
                if has_rf {
                    let targets: Vec<&str> = rem[1..]
                        .iter()
                        .copied()
                        .filter(|a| !a.starts_with('-'))
                        .collect();
                    let all_safe_caches = !targets.is_empty()
                        && targets.iter().all(|t| {
                            let clean = t.trim_start_matches("./").trim_end_matches('/');
                            let root_seg = clean.split('/').next().unwrap_or(clean);
                            matches!(
                                root_seg,
                                "dist"
                                    | "node_modules"
                                    | "target"
                                    | ".next"
                                    | "build"
                                    | "coverage"
                                    | ".turbo"
                                    | ".vite"
                            )
                        });
                    if !all_safe_caches {
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
