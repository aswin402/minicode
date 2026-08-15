use std::path::Path;
use std::process::Command;

const WHITELIST_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "USER", "LANG", "LC_ALL", "TERM", "SHELL", "EDITOR", "TMPDIR", "PWD",
];

const SECRET_PATTERNS: &[&str] = &[
    "KEY",
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "AUTH",
    "BEARER",
    "PRIVATE",
    "SIGNING",
    "CERTIFICATE",
    "DATABASE_URL",
    "CONN_STR",
    "DSN",
    "SSH_AUTH_SOCK",
    "KUBECONFIG",
    "DOCKER_HOST",
];

const BLOCKED_PREFIXES: &[&str] = &[
    "AWS_",
    "GITHUB_",
    "OPENAI_",
    "GEMINI_",
    "ANTHROPIC_",
    "DEEPSEEK_",
    "MISTRAL_",
    "GROQ_",
    "COHERE_",
    "OLLAMA_",
    "CLERK_",
    "SUPABASE_",
    "FIREBASE_",
    "SENTRY_",
    "VERCEL_",
    "NETLIFY_",
    "HEROKU_",
    "DIGITALOCEAN_",
    "CLOUDFLARE_",
];

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

    // 2. Scan all current environment variables, filtering out any secret patterns
    for (key, val) in std::env::vars() {
        let key_upper = key.to_uppercase();
        let is_sensitive = SECRET_PATTERNS.iter().any(|&pat| key_upper.contains(pat));
        let has_blocked_prefix = BLOCKED_PREFIXES
            .iter()
            .any(|&pfx| key_upper.starts_with(pfx));

        // Only allow non-sensitive, non-duplicate variables
        if !is_sensitive && !has_blocked_prefix && !WHITELIST_ENV_VARS.contains(&key.as_str()) {
            cmd.env(key, val);
        }
    }

    // 3. Set workspace and sandbox indicator variables
    cmd.env("MINICODE_WORKSPACE", workspace);
    cmd.env("MINICODE_SANDBOX", "1");

    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitized_command_env() {
        std::env::set_var("TEST_SECRET_API_KEY", "super_secret_value");
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
        assert_eq!(env_map.get("MINICODE_SANDBOX"), Some(&"1".to_string()));
    }
}
