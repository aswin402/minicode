use crate::constants::{BLOCKED_PREFIXES, REDACTED_PLACEHOLDER, SECRET_PATTERNS};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

static GLOBAL_REDACTOR: OnceLock<SecretRedactor> = OnceLock::new();

/// Minimum Shannon entropy (bits per byte) for unstructured secret value detection.
const SECRET_SHANNON_ENTROPY_MIN_BITS: f64 = 4.10;
/// Minimum candidate value length for entropy-based secret redaction.
const SECRET_ENTROPY_MIN_LEN: usize = 20;

/// Compiled regex pattern with descriptive labels for both compact and typed redaction modes.
struct RedactionPattern {
    regex: Regex,
    label: &'static str,
    replacement: &'static str,
    typed_replacement: &'static str,
    warning_msg: &'static str,
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

        // Phase 1: Apply regex patterns (zero allocation when Cow::Borrowed)
        for pattern in &self.patterns {
            if let std::borrow::Cow::Owned(redacted) =
                pattern.regex.replace_all(&output, pattern.replacement)
            {
                tracing::debug!(pattern = pattern.label, "Redacted secret match");
                output = redacted;
            }
        }

        // Phase 2: Shannon byte-entropy pass for novel/unlisted provider tokens in secret assignments
        if let Some(entropy_redacted) =
            Self::redact_high_entropy_assignments(&output, REDACTED_PLACEHOLDER)
        {
            tracing::debug!("Redacted high-entropy credential assignment");
            output = entropy_redacted;
        }

        // Phase 3: Exact-match env secret replacement
        // Only replace values that are long enough to avoid false positives
        for secret_val in &self.env_secrets {
            if secret_val.len() >= 8 && output.contains(secret_val.as_str()) {
                output = output.replace(secret_val.as_str(), REDACTED_PLACEHOLDER);
                tracing::debug!("Redacted exact-match environment secret");
            }
        }

        output
    }

    /// Scans content and returns both typed-marker redacted content and `(1-indexed line, warning)` entries.
    pub fn redact_detailed(&self, content: &str) -> (String, Vec<(usize, String)>) {
        if content.is_empty() {
            return (String::new(), Vec::new());
        }

        let mut working = content.to_string();
        let mut warnings = Vec::new();

        // Multi-line PEM private key pass first
        if let Some(pem_pat) = self
            .patterns
            .iter()
            .find(|p| p.label == "private_key_block")
        {
            if let std::borrow::Cow::Owned(redacted) = pem_pat
                .regex
                .replace_all(&working, pem_pat.typed_replacement)
            {
                warnings.push((1, pem_pat.warning_msg.to_string()));
                working = redacted;
            }
        }

        let mut out_lines: Vec<String> = Vec::new();
        for (idx, line) in working.lines().enumerate() {
            let line_num = idx + 1;
            let mut current = line.to_string();

            for pattern in &self.patterns {
                if pattern.label == "private_key_block" {
                    continue;
                }
                if let std::borrow::Cow::Owned(redacted) = pattern
                    .regex
                    .replace_all(&current, pattern.typed_replacement)
                {
                    current = redacted;
                    warnings.push((line_num, pattern.warning_msg.to_string()));
                }
            }

            if let Some(entropy_redacted) =
                Self::redact_high_entropy_assignments(&current, "[REDACTED-SECRET]")
            {
                current = entropy_redacted;
                warnings.push((
                    line_num,
                    "High-entropy credential/secret value detected".to_string(),
                ));
            }

            for secret_val in &self.env_secrets {
                if secret_val.len() >= 8 && current.contains(secret_val.as_str()) {
                    current = current.replace(secret_val.as_str(), "[REDACTED-SECRET]");
                    warnings.push((line_num, "Environment secret value detected".to_string()));
                }
            }

            out_lines.push(current);
        }

        let final_content = if warnings.is_empty() {
            content.to_string()
        } else {
            let mut joined = out_lines.join("\n");
            if content.ends_with('\n') && !joined.ends_with('\n') {
                joined.push('\n');
            }
            joined
        };

        (final_content, warnings)
    }

    /// Computes Shannon byte entropy $H(X) = -\sum p(b) \log_2 p(b)$ in bits per byte.
    #[must_use]
    pub fn shannon_entropy(s: &str) -> f64 {
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return 0.0;
        }
        let mut counts = [0u32; 256];
        for &b in bytes {
            counts[b as usize] += 1;
        }
        let len = bytes.len() as f64;
        let mut entropy = 0.0;
        for &c in &counts {
            if c > 0 {
                let p = f64::from(c) / len;
                entropy -= p * p.log2();
            }
        }
        entropy
    }

    fn is_secret_key_identifier(lhs: &str) -> bool {
        let key = lhs
            .trim()
            .trim_matches(|c: char| matches!(c, '"' | '\'' | '`'))
            .rsplit(|c: char| c.is_whitespace() || matches!(c, '.' | '{' | ',' | ';'))
            .next()
            .unwrap_or("")
            .trim_matches(|c: char| matches!(c, '"' | '\'' | '`'))
            .to_ascii_lowercase();
        // Scoped package names or path keys (e.g. `@radix-ui/react-one-time-password-field`) are not secret identifiers
        if key.is_empty() || key.starts_with('@') || key.contains('/') {
            return false;
        }
        [
            "secret",
            "token",
            "apikey",
            "api_key",
            "api-key",
            "private_key",
            "private-key",
            "client_key",
            "signing_key",
            "encryption_key",
            "master_key",
            "passwd",
            "password",
            "credential",
            "webhook_key",
        ]
        .iter()
        .any(|cue| key.contains(cue))
    }

    fn is_high_entropy_secret_value(val: &str) -> bool {
        if val.len() < SECRET_ENTROPY_MIN_LEN
            || val.contains("[REDACTED")
            || val.contains("://")
            || val.starts_with('/')
            || val.starts_with("./")
            || val.starts_with("../")
            || val.starts_with('[')
            || val.starts_with('{')
            || val.starts_with('@')
            || val.starts_with("sha256-")
            || val.starts_with("sha384-")
            || val.starts_with("sha512-")
            || val.chars().any(char::is_whitespace)
        {
            return false;
        }
        // Skip standard UUIDs (8-4-4-4-12 hex)
        if val.len() == 36
            && val.bytes().enumerate().all(|(i, b)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
        {
            return false;
        }
        let has_lower = val.bytes().any(|b| b.is_ascii_lowercase());
        let has_upper = val.bytes().any(|b| b.is_ascii_uppercase());
        let has_digit = val.bytes().any(|b| b.is_ascii_digit());
        let class_count = usize::from(has_lower) + usize::from(has_upper) + usize::from(has_digit);
        class_count >= 2 && Self::shannon_entropy(val) >= SECRET_SHANNON_ENTROPY_MIN_BITS
    }

    /// Scans lines for `key = "high_entropy_value"` or `key: "high_entropy_value"` assignments
    /// where the LHS names a credential field and the RHS exhibits high Shannon entropy.
    fn redact_high_entropy_assignments(input: &str, marker: &str) -> Option<String> {
        let mut changed = false;
        let mut out = String::with_capacity(input.len());

        for (idx, line) in input.split('\n').enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            if let Some(sep_pos) = line.find(['=', ':']) {
                let lhs = &line[..sep_pos];
                let rhs = &line[sep_pos + 1..];
                if Self::is_secret_key_identifier(lhs) {
                    let rhs_trimmed = rhs.trim_start();
                    let ws_len = rhs.len() - rhs_trimmed.len();
                    if let Some(quote) = rhs_trimmed
                        .chars()
                        .next()
                        .filter(|&c| c == '"' || c == '\'')
                    {
                        let inner = &rhs_trimmed[quote.len_utf8()..];
                        if let Some(end_q) = inner.find(quote) {
                            let candidate = &inner[..end_q];
                            if Self::is_high_entropy_secret_value(candidate) {
                                out.push_str(&line[..=sep_pos]);
                                out.push_str(&rhs[..ws_len]);
                                out.push(quote);
                                out.push_str(marker);
                                out.push(quote);
                                out.push_str(&inner[end_q + quote.len_utf8()..]);
                                changed = true;
                                continue;
                            }
                        }
                    } else {
                        let val_end = rhs_trimmed
                            .find(|c: char| c.is_whitespace() || matches!(c, ';' | ',' | ')' | '}'))
                            .unwrap_or(rhs_trimmed.len());
                        let candidate = &rhs_trimmed[..val_end];
                        if Self::is_high_entropy_secret_value(candidate) {
                            out.push_str(&line[..=sep_pos]);
                            out.push_str(&rhs[..ws_len]);
                            out.push_str(marker);
                            out.push_str(&rhs_trimmed[val_end..]);
                            changed = true;
                            continue;
                        }
                    }
                }
            }
            out.push_str(line);
        }

        changed.then_some(out)
    }

    /// Compiles all regex patterns for common secret formats.
    fn build_patterns() -> Vec<RedactionPattern> {
        let mut patterns = Vec::with_capacity(20);

        // Helper: compile a regex pattern, skipping on error (should never happen with static patterns)
        let mut add = |label: &'static str,
                       pat: &str,
                       replacement: &'static str,
                       typed_replacement: &'static str,
                       warning_msg: &'static str| match Regex::new(pat) {
            Ok(regex) => patterns.push(RedactionPattern {
                regex,
                label,
                replacement,
                typed_replacement,
                warning_msg,
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
            "[REDACTED: PRIVATE KEY]",
            "PEM Private Key detected",
        );

        // --- Anthropic & MiniMax Keys (must precede generic sk- OpenAI key) ---
        add(
            "anthropic_key",
            r"sk-ant-[a-zA-Z0-9\-_]{20,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-ANTHROPIC-KEY]",
            "Anthropic API Key detected",
        );
        add(
            "minimax_key",
            r"sk-cp-[a-zA-Z0-9\-_]{32,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-MINIMAX-KEY]",
            "MiniMax API Key detected",
        );

        // --- OpenAI Keys ---
        add(
            "openai_project_key",
            r"sk-proj-[a-zA-Z0-9\-_]{20,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-OPENAI-KEY]",
            "OpenAI API Key detected",
        );
        add(
            "openai_key",
            r"sk-[a-zA-Z0-9]{20,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-OPENAI-KEY]",
            "OpenAI API Key detected",
        );

        // --- GitHub Tokens ---
        add(
            "github_fine_grained",
            r"github_pat_[A-Za-z0-9_]{22,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-GITHUB-PAT]",
            "GitHub PAT detected",
        );
        add(
            "github_token",
            r"gh[ps]_[A-Za-z0-9_]{36,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-GITHUB-PAT]",
            "GitHub PAT detected",
        );

        // --- AWS Keys ---
        add(
            "aws_access_key",
            r"AKIA[0-9A-Z]{16}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-AWS-ACCESS-KEY-ID]",
            "AWS Access Key ID detected",
        );
        add(
            "aws_secret_key",
            r#"(?i)aws[_\s]*(?:secret[_\s]*(?:access[_\s]*)?)?key\s*[=:]\s*['"]?[0-9a-zA-Z/+=]{40}['"]?"#,
            REDACTED_PLACEHOLDER,
            "[REDACTED-AWS-SECRET-KEY]",
            "AWS Secret Access Key detected",
        );

        // --- Google / Gemini API Key ---
        add(
            "google_api_key",
            r"AIza[0-9A-Za-z\-_]{35}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-GEMINI-KEY]",
            "Gemini/Google API Key detected",
        );

        // --- Stripe Keys ---
        add(
            "stripe_key",
            r"[rs]k_(?:test|live)_[0-9a-zA-Z]{24,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-STRIPE-KEY]",
            "Stripe API Key detected",
        );

        // --- Slack Tokens ---
        add(
            "slack_token",
            r"xox[bpras]-[0-9]{10,}-[a-zA-Z0-9]+",
            REDACTED_PLACEHOLDER,
            "[REDACTED-SLACK-TOKEN]",
            "Slack Token detected",
        );

        // --- JWT / Bearer Tokens ---
        add(
            "bearer_token",
            r"Bearer\s+[A-Za-z0-9\-._~+/]{16,}=*",
            "Bearer [REDACTED]",
            "Bearer [REDACTED]",
            "Bearer Token detected",
        );
        add(
            "jwt_token",
            r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+",
            REDACTED_PLACEHOLDER,
            "[REDACTED-JWT]",
            "JWT Token detected",
        );

        // --- Generic Key=Value Assignments ---
        add(
            "generic_key_value",
            r#"(?i)(?:api[_\-]?key|api[_\-]?secret|api[_\-]?token|access[_\-]?token|auth[_\-]?token|secret[_\-]?key|client[_\-]?secret|client[_\-]?key|db[_\-]?password)\s*[=:]\s*['"]?[A-Za-z0-9\-._~+/]{16,}['"]?"#,
            REDACTED_PLACEHOLDER,
            "[REDACTED-SECRET]",
            "Potential generic credential/token detected",
        );

        // --- Connection String Passwords ---
        add(
            "connection_password",
            r"(?i)(?:password|pwd)\s*=\s*[^;\s&\[]\S{7,}",
            REDACTED_PLACEHOLDER,
            "[REDACTED-SECRET]",
            "Connection password detected",
        );

        // --- Hex Secrets ---
        add(
            "hex_secret",
            "(?i)(?:secret|token|credential)\\s*[=:]\\s*['\"]?[0-9a-f]{32,}['\"]?",
            REDACTED_PLACEHOLDER,
            "[REDACTED-SECRET]",
            "Hex secret detected",
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
            "Authorization: Bearer ghp_********************************",
            "Pass a Bearer token in the Authorization header",
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

    #[test]
    fn test_shannon_entropy_redacts_unlisted_provider_secret() {
        let r = SecretRedactor::patterns_only();
        let input = "export CUSTOM_WEBHOOK_SECRET=\"v9K2mP8xQ4rT7wZ1bN6jL3hF5dS0aG8eY2uI!\"";
        let output = r.redact(input);
        assert!(
            !output.contains("v9K2mP8xQ4rT7wZ1"),
            "High-entropy secret assignment should be redacted: {}",
            output
        );
        assert!(output.contains(REDACTED_PLACEHOLDER));
    }

    #[test]
    fn test_scoped_npm_password_package_and_sri_hash_not_redacted_as_secret() {
        let r = SecretRedactor::patterns_only();
        let bun_lock_snippet = r#"  "@radix-ui/react-one-time-password-field": ["@radix-ui/react-one-time-password-field@0.1.8", "", { "dependencies": { "@radix-ui/react-primitive": "2.1.4" } }, "sha512-v9K2mP8xQ4rT7wZ1bN6jL3hF5dS0aG8eY2uI=="],"#;
        assert_eq!(
            r.redact(bun_lock_snippet),
            bun_lock_snippet,
            "bun.lock scoped package entry with 'password' in package name must not false-trigger secret redaction"
        );
    }
}
