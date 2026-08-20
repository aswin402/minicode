use crate::constants::{BLOCKED_PREFIXES, REDACTED_PLACEHOLDER, SECRET_PATTERNS};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

static GLOBAL_REDACTOR: OnceLock<SecretRedactor> = OnceLock::new();

/// Compiled regex pattern with a descriptive label for tracing
struct RedactionPattern {
    regex: Regex,
    label: &'static str,
    replacement: &'static str,
}

/// Zero-leak secret redaction engine.
///
/// Combines regex-based pattern detection for common API key formats with
/// exact-match harvesting of environment variable values that match known
/// secret patterns.
pub struct SecretRedactor {
    patterns: Vec<RedactionPattern>,
    /// Exact secret values harvested from the host environment at init time
    env_secrets: HashSet<String>,
}

impl SecretRedactor {
    /// Returns the process-global redactor instance, initializing on first call.
    pub fn global() -> &'static SecretRedactor {
        GLOBAL_REDACTOR.get_or_init(Self::new)
    }

    /// Constructs a new redactor with compiled patterns and harvested env secrets.
    fn new() -> Self {
        let patterns = Self::build_patterns();
        let env_secrets = Self::harvest_env_secrets();

        tracing::info!(
            pattern_count = patterns.len(),
            env_secret_count = env_secrets.len(),
            "SecretRedactor initialized"
        );

        Self {
            patterns,
            env_secrets,
        }
    }

    /// Creates a redactor with only regex patterns (no env harvesting).
    /// Useful for testing without side effects on the process environment.
    #[cfg(test)]
    pub fn patterns_only() -> Self {
        Self {
            patterns: Self::build_patterns(),
            env_secrets: HashSet::new(),
        }
    }

    /// Creates a redactor with custom exact-match secrets for testing.
    #[cfg(test)]
    pub fn with_secrets(secrets: HashSet<String>) -> Self {
        Self {
            patterns: Self::build_patterns(),
            env_secrets: secrets,
        }
    }

    /// Apply all redaction rules to the input string.
    ///
    /// Order of operations:
    /// 1. Private key block replacement (multi-line)
    /// 2. Regex pattern matching (single-line tokens)
    /// 3. Exact-match env secret replacement
    pub fn redact(&self, input: &str) -> String {
        if input.is_empty() {
            return String::new();
        }

        let mut output = input.to_string();

        // Phase 1: Apply regex patterns
        for pattern in &self.patterns {
            let before_len = output.len();
            output = pattern
                .regex
                .replace_all(&output, pattern.replacement)
                .to_string();
            if output.len() != before_len {
                tracing::debug!(pattern = pattern.label, "Redacted secret match");
            }
        }

        // Phase 2: Exact-match env secret replacement
        // Only replace values that are long enough to avoid false positives
        for secret_val in &self.env_secrets {
            if secret_val.len() >= 8 && output.contains(secret_val.as_str()) {
                output = output.replace(secret_val.as_str(), REDACTED_PLACEHOLDER);
                tracing::debug!("Redacted exact-match environment secret");
            }
        }

        output
    }

    /// Compiles all regex patterns for common secret formats.
    fn build_patterns() -> Vec<RedactionPattern> {
        let mut patterns = Vec::with_capacity(20);

        // Helper: compile a regex pattern, skipping on error (should never happen with static patterns)
        let mut add = |label: &'static str, pat: &str, replacement: &'static str| match Regex::new(
            pat,
        ) {
            Ok(regex) => patterns.push(RedactionPattern {
                regex,
                label,
                replacement,
            }),
            Err(e) => {
                tracing::error!(pattern = label, error = %e, "Failed to compile redaction regex");
            }
        };

        // --- Private Key Blocks (multi-line, must come first) ---
        add(
            "private_key_block",
            r"-----BEGIN\s+(?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----[\s\S]*?-----END\s+(?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
            "[REDACTED: PRIVATE KEY]",
        );

        // --- OpenAI Keys ---
        add(
            "openai_project_key",
            r"sk-proj-[a-zA-Z0-9\-_]{20,}",
            REDACTED_PLACEHOLDER,
        );
        add("openai_key", r"sk-[a-zA-Z0-9]{20,}", REDACTED_PLACEHOLDER);

        // --- Anthropic Keys ---
        add(
            "anthropic_key",
            r"sk-ant-[a-zA-Z0-9\-_]{20,}",
            REDACTED_PLACEHOLDER,
        );

        // --- GitHub Tokens ---
        add(
            "github_fine_grained",
            r"github_pat_[A-Za-z0-9_]{22,}",
            REDACTED_PLACEHOLDER,
        );
        add(
            "github_token",
            r"gh[ps]_[A-Za-z0-9_]{36,}",
            REDACTED_PLACEHOLDER,
        );

        // --- AWS Keys ---
        add("aws_access_key", r"AKIA[0-9A-Z]{16}", REDACTED_PLACEHOLDER);
        add(
            "aws_secret_key",
            r#"(?i)aws[_\s]*(?:secret[_\s]*(?:access[_\s]*)?)?key\s*[=:]\s*['"]?[0-9a-zA-Z/+]{40}['"]?"#,
            REDACTED_PLACEHOLDER,
        );

        // --- Google API Key ---
        add(
            "google_api_key",
            r"AIza[0-9A-Za-z\-_]{35}",
            REDACTED_PLACEHOLDER,
        );

        // --- Stripe Keys ---
        add(
            "stripe_key",
            r"[rs]k_(?:test|live)_[0-9a-zA-Z]{24,}",
            REDACTED_PLACEHOLDER,
        );

        // --- Slack Tokens ---
        add(
            "slack_token",
            r"xox[bpras]-[0-9]{10,}-[a-zA-Z0-9]+",
            REDACTED_PLACEHOLDER,
        );

        // --- JWT / Bearer Tokens ---
        add(
            "bearer_token",
            r"Bearer\s+[A-Za-z0-9\-._~+/]+=*",
            "Bearer [REDACTED]",
        );
        add(
            "jwt_token",
            r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+",
            REDACTED_PLACEHOLDER,
        );

        // --- Generic Key=Value Assignments ---
        add(
            "generic_key_value",
            r#"(?i)(?:api[_\-]?key|api[_\-]?secret|access[_\-]?token|auth[_\-]?token|secret[_\-]?key)\s*[=:]\s*['"]?[A-Za-z0-9\-._~+/]{16,}['"]?"#,
            REDACTED_PLACEHOLDER,
        );

        // --- Connection String Passwords ---
        add(
            "connection_password",
            r"(?i)(?:password|pwd)\s*=\s*[^;\s&]{8,}",
            REDACTED_PLACEHOLDER,
        );

        // --- Hex Secrets ---
        add(
            "hex_secret",
            "(?i)(?:secret|token|credential)\\s*[=:]\\s*['\"]?[0-9a-f]{32,}['\"]?",
            REDACTED_PLACEHOLDER,
        );

        patterns
    }

    /// Harvests actual secret values from the host environment.
    ///
    /// Scans all env vars and collects values of those whose key matches
    /// `SECRET_PATTERNS` or `BLOCKED_PREFIXES` from `constants.rs`.
    fn harvest_env_secrets() -> HashSet<String> {
        let mut secrets = HashSet::new();

        for (key, val) in std::env::vars() {
            if val.len() < 8 {
                continue; // Skip short values to avoid false positives
            }

            let key_upper = key.to_uppercase();
            let is_sensitive = SECRET_PATTERNS.iter().any(|&pat| key_upper.contains(pat));
            let has_blocked_prefix = BLOCKED_PREFIXES
                .iter()
                .any(|&pfx| key_upper.starts_with(pfx));

            if is_sensitive || has_blocked_prefix {
                secrets.insert(val);
            }
        }

        secrets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_openai_key() {
        let r = SecretRedactor::patterns_only();
        let input = "Using key sk-abc123def456ghi789jkl012mno345";
        let output = r.redact(input);
        assert!(
            !output.contains("sk-abc123"),
            "OpenAI key should be redacted"
        );
        assert!(output.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_redact_openai_project_key() {
        let r = SecretRedactor::patterns_only();
        let input = "key: sk-proj-abc123def456ghi789jkl012mno345pqr678";
        let output = r.redact(input);
        assert!(
            !output.contains("sk-proj-abc123"),
            "OpenAI project key should be redacted"
        );
        assert!(output.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_redact_anthropic_key() {
        let r = SecretRedactor::patterns_only();
        let input = "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghij1234567890abcdefghij";
        let output = r.redact(input);
        assert!(
            !output.contains("sk-ant-api03"),
            "Anthropic key should be redacted"
        );
    }

    #[test]
    fn test_redact_github_token() {
        let r = SecretRedactor::patterns_only();
        let input = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij1234";
        let output = r.redact(input);
        assert!(
            !output.contains("ghp_ABCDEF"),
            "GitHub token should be redacted"
        );
        assert!(output.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_redact_github_fine_grained() {
        let r = SecretRedactor::patterns_only();
        let input = "github_pat_22ABCDEFGHIJKLMNOPQRST_abcdefghij";
        let output = r.redact(input);
        assert!(
            !output.contains("github_pat_22ABC"),
            "GitHub fine-grained token should be redacted"
        );
    }

    #[test]
    fn test_redact_aws_access_key() {
        let r = SecretRedactor::patterns_only();
        let input = "aws_access_key_id = AKIAIOSFODNN7EXAMPLE";
        let output = r.redact(input);
        assert!(
            !output.contains("AKIAIOSFODNN7EXAMPLE"),
            "AWS access key should be redacted"
        );
        assert!(output.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_redact_bearer_token() {
        let r = SecretRedactor::patterns_only();
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig";
        let output = r.redact(input);
        assert!(
            !output.contains("eyJhbGciOi"),
            "Bearer token should be redacted"
        );
        assert!(output.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn test_redact_private_key_block() {
        let r = SecretRedactor::patterns_only();
        let input = r#"Found key:
-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF8PbnGy0AHB7MhgHcTz6sE2I2yGP
base64encodedkeydata
-----END RSA PRIVATE KEY-----
Done."#;
        let output = r.redact(input);
        assert!(
            !output.contains("MIIEpAIBAAK"),
            "Private key content should be redacted"
        );
        assert!(output.contains("[REDACTED: PRIVATE KEY]"));
        assert!(output.contains("Found key:"));
        assert!(output.contains("Done."));
    }

    #[test]
    fn test_redact_jwt_token() {
        let r = SecretRedactor::patterns_only();
        let input = "token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let output = r.redact(input);
        assert!(
            !output.contains("eyJhbGciOi"),
            "JWT token should be redacted"
        );
    }

    #[test]
    fn test_redact_google_api_key() {
        let r = SecretRedactor::patterns_only();
        let input = "GOOGLE_API_KEY=AIzaSyB-abcdefghijklmnopqrstuvwxyz12345";
        let output = r.redact(input);
        assert!(
            !output.contains("AIzaSyB-abc"),
            "Google API key should be redacted"
        );
    }

    #[test]
    fn test_redact_stripe_key() {
        let r = SecretRedactor::patterns_only();
        // Use rk_test_ prefix which is less likely to trigger GitHub push protection
        let input = "rk_test_FAKE000000000000000000000000";
        let output = r.redact(input);
        assert!(
            !output.contains("rk_test_FAKE"),
            "Stripe key should be redacted"
        );
    }

    #[test]
    fn test_redact_slack_token() {
        let r = SecretRedactor::patterns_only();
        // Use xoxa- prefix variant
        let input = "SLACK_BOT_TOKEN=xoxa-9999999999999-FAKE000000token";
        let output = r.redact(input);
        assert!(
            !output.contains("xoxa-99999"),
            "Slack token should be redacted"
        );
    }

    #[test]
    fn test_redact_connection_password() {
        let r = SecretRedactor::patterns_only();
        let input = "Server=myserver;Database=mydb;Password=SuperSecretP@ss123;User=admin";
        let output = r.redact(input);
        assert!(
            !output.contains("SuperSecretP@ss123"),
            "Connection password should be redacted"
        );
    }

    #[test]
    fn test_redact_generic_api_key_assignment() {
        let r = SecretRedactor::patterns_only();
        let input = "api_key = \"abcdef0123456789abcdef01\"";
        let output = r.redact(input);
        assert!(
            !output.contains("abcdef0123456789"),
            "Generic API key assignment should be redacted"
        );
    }

    #[test]
    fn test_redact_env_exact_match() {
        let mut secrets = HashSet::new();
        secrets.insert("my-super-secret-database-password-2024".to_string());
        let r = SecretRedactor::with_secrets(secrets);

        let input = "Connected with password my-super-secret-database-password-2024 to db";
        let output = r.redact(input);
        assert!(
            !output.contains("my-super-secret-database-password-2024"),
            "Exact-match env secret should be redacted"
        );
        assert!(output.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_no_false_positive_normal_output() {
        let r = SecretRedactor::patterns_only();

        let normal_outputs = vec![
            "Compiling minicode v0.0.36",
            "test result: ok. 171 passed; 0 failed",
            "src/main.rs:42: fn main() {",
            "Hello, world! This is a normal string.",
            "The variable `key` was not found in scope.",
            "Running 5 tests...",
            "File created at /tmp/output.txt",
            "HTTP 200 OK - Content-Type: application/json",
        ];

        for input in normal_outputs {
            let output = r.redact(input);
            assert_eq!(
                input, output,
                "Normal output should not be modified: {}",
                input
            );
        }
    }

    #[test]
    fn test_redact_empty_input() {
        let r = SecretRedactor::patterns_only();
        assert_eq!(r.redact(""), "");
    }

    #[test]
    fn test_redact_multiple_secrets_in_one_string() {
        let r = SecretRedactor::patterns_only();
        let input = "Keys: sk-abc123def456ghi789jkl012mno345 and ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij1234";
        let output = r.redact(input);
        assert!(
            !output.contains("sk-abc123"),
            "First secret should be redacted"
        );
        assert!(
            !output.contains("ghp_ABCDEF"),
            "Second secret should be redacted"
        );
    }

    #[test]
    fn test_env_secret_min_length() {
        let mut secrets = HashSet::new();
        secrets.insert("short".to_string()); // 5 chars, should be skipped
        secrets.insert("longEnoughSecret".to_string()); // 16 chars, should match
        let r = SecretRedactor::with_secrets(secrets);

        let input = "Value is short and also longEnoughSecret here";
        let output = r.redact(input);
        assert!(
            output.contains("short"),
            "Short secret should not be redacted (false positive guard)"
        );
        assert!(
            !output.contains("longEnoughSecret"),
            "Long enough secret should be redacted"
        );
    }
}
