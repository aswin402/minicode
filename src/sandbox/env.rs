use crate::constants::{BLOCKED_PREFIXES, SECRET_PATTERNS, WHITELIST_ENV_VARS};
use std::path::Path;
use std::process::Command;

/// Builds a sanitized `std::process::Command` that clears all inherited variables
/// and selectively restores only safe environment variables.
pub fn build_sanitized_command(program: &str, workspace: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear();
    cmd.current_dir(workspace);

    // 1. Pass through explicitly whitelisted variables if they exist in host environment
    for &var_name in WHITELIST_ENV_VARS {
        if let Ok(val) = std::env::var(var_name) {
            cmd.env(var_name, val);
        }
    }

    // 2. Scan all current environment variables, filtering out any secret patterns or credential URIs
    for (key, val) in std::env::vars() {
        let key_upper = key.to_uppercase();
        let is_sensitive = SECRET_PATTERNS.iter().any(|&pat| key_upper.contains(pat));
        let has_blocked_prefix = BLOCKED_PREFIXES
            .iter()
            .any(|&pfx| key_upper.starts_with(pfx));
        let has_credential_uri = val.contains("://") && val.contains('@');

        // Only allow non-sensitive, non-duplicate variables
        if !is_sensitive
            && !has_blocked_prefix
            && !has_credential_uri
            && !WHITELIST_ENV_VARS.contains(&key.as_str())
        {
            cmd.env(key, val);
        }
    }

    // 3. Set workspace and sandbox indicator variables
    cmd.env("MINICODE_WORKSPACE", workspace);
    cmd.env("MINICODE_SANDBOX", "1");

    // 4. Ensure package managers like Bun have consistent temp and install paths on the same mount
    if let Some(home) = dirs::home_dir() {
        let bun_dir = home.join(".bun");
        if std::env::var("BUN_INSTALL").is_err() && bun_dir.exists() {
            cmd.env("BUN_INSTALL", &bun_dir);
        }
        if std::env::var("BUN_TMPDIR").is_err() {
            let bun_tmp = bun_dir.join("tmp");
            let _ = std::fs::create_dir_all(&bun_tmp);
            cmd.env("BUN_TMPDIR", &bun_tmp);
        }
    }

    // 5. If running inside an isolated `.minicode/worktrees/<id>` directory, share the parent
    // repository's `target/` directory so Rust/Cargo subagents reuse warm build artifacts
    // instead of triggering multi-GB cold rebuilds.
    if std::env::var("CARGO_TARGET_DIR").is_err() {
        if let Some(repo_root) = detect_parent_repo_from_worktree(workspace) {
            if repo_root.join("Cargo.toml").exists() || repo_root.join("target").exists() {
                let shared_target = repo_root.join("target");
                let _ = std::fs::create_dir_all(&shared_target);
                cmd.env("CARGO_TARGET_DIR", shared_target);
            }
        }
    }

    cmd
}

/// If `workspace` is `<repo_root>/.minicode/worktrees/<worktree_name>`, returns `Some(<repo_root>)`.
pub fn detect_parent_repo_from_worktree(workspace: &Path) -> Option<std::path::PathBuf> {
    let worktrees_dir = workspace.parent()?;
    if worktrees_dir.file_name()?.to_str()? != crate::constants::WORKTREES_DIR_NAME {
        return None;
    }
    let minicode_dir = worktrees_dir.parent()?;
    if minicode_dir.file_name()?.to_str()? != crate::constants::WORKSPACE_DIR_NAME {
        return None;
    }
    minicode_dir.parent().map(|p| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitized_command_env() {
        std::env::set_var("TEST_SECRET_API_KEY", "super_secret_value");
        std::env::set_var("DATABASE_URL", "postgres://admin:secret@localhost:5432/db");
        std::env::set_var("STRIPE_KEY", "sk_live_12345");
        std::env::set_var("LD_PRELOAD", "/tmp/malicious.so");
        std::env::set_var("CUSTOM_SERVICE_URI", "https://user:pass@internal.host/api");
        std::env::set_var("TEST_SAFE_VAR", "safe_value");

        let temp_dir = std::env::temp_dir();
        let cmd = build_sanitized_command("echo", &temp_dir);

        let envs: Vec<(&std::ffi::OsStr, Option<&std::ffi::OsStr>)> = cmd.get_envs().collect();
        let env_map: std::collections::HashMap<String, String> = envs
            .into_iter()
            .filter_map(|(k, v)| {
                v.map(|val| {
                    (
                        k.to_string_lossy().to_string(),
                        val.to_string_lossy().to_string(),
                    )
                })
            })
            .collect();

        assert!(!env_map.contains_key("TEST_SECRET_API_KEY"));
        assert!(!env_map.contains_key("DATABASE_URL"));
        assert!(!env_map.contains_key("STRIPE_KEY"));
        assert!(!env_map.contains_key("LD_PRELOAD"));
        assert!(!env_map.contains_key("CUSTOM_SERVICE_URI"));
        assert_eq!(env_map.get("MINICODE_SANDBOX"), Some(&"1".to_string()));
    }
}
