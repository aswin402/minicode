use regex::Regex;
use std::sync::OnceLock;

/// Result of a secret redaction pass over text or code.
#[derive(Debug, Clone)]
pub struct RedactResult {
    pub content: String,
    pub warnings: Vec<(usize, String)>,
}

struct SecretRegexes {
    openai_re: Regex,
    anthropic_re: Regex,
    gemini_re: Regex,
    github_re: Regex,
    aws_id_re: Regex,
    aws_secret_re: Regex,
    stripe_re: Regex,
    minimax_re: Regex,
    generic_key_re: Regex,
}

fn get_secret_regexes() -> Option<&'static SecretRegexes> {
    static REGEXES: OnceLock<Option<SecretRegexes>> = OnceLock::new();
    REGEXES
        .get_or_init(|| {
            let openai_re = Regex::new(r#"sk-(?:proj-)?[a-zA-Z0-9_\-]{32,120}"#).ok()?;
            let anthropic_re =
                Regex::new(r#"sk-ant-(?:api03-)?[a-zA-Z0-9_\-]{32,120}"#).ok()?;
            let gemini_re = Regex::new(r#"AIzaSy[a-zA-Z0-9_\-]{33}"#).ok()?;
            let github_re =
                Regex::new(r#"ghp_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{82}"#).ok()?;
            let aws_id_re = Regex::new(r#"AKIA[0-9A-Z]{16}"#).ok()?;
            let aws_secret_re = Regex::new(
                r#"(?i)(aws_secret_access_key\s*[:=]\s*['\ns]?[']?)[a-zA-Z0-9/+=]{40}(['\ns]?[']?)"#,
            )
            .ok()?;
            let stripe_re = Regex::new(r#"sk_live_[0-9a-zA-Z]{24,32}"#).ok()?;
            let minimax_re = Regex::new(r#"sk-cp-[a-zA-Z0-9_\-]{60,140}"#).ok()?;
            let generic_key_re = Regex::new(
                r#"(?i)(api_key|api_token|client_secret|client_key|secret_key|db_password)(\s*[:=]\s*['"])[a-zA-Z0-9_\-]{32,96}(['"])"#,
            )
            .ok()?;

            Some(SecretRegexes {
                openai_re,
                anthropic_re,
                gemini_re,
                github_re,
                aws_id_re,
                aws_secret_re,
                stripe_re,
                minimax_re,
                generic_key_re,
            })
        })
        .as_ref()
}

/// Scans content for secrets and replaces them with standard redaction markers.
pub fn redact_secrets(content: &str) -> RedactResult {
    let re = match get_secret_regexes() {
        Some(r) => r,
        None => {
            return RedactResult {
                content: content.to_string(),
                warnings: Vec::new(),
            }
        }
    };

    let mut warnings = Vec::new();
    let mut lines = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let mut new_line = line.to_string();

        if re.anthropic_re.is_match(&new_line) {
            new_line = re
                .anthropic_re
                .replace_all(&new_line, "[REDACTED-ANTHROPIC-KEY]")
                .to_string();
            warnings.push((line_num, "Anthropic API Key detected".to_string()));
        }

        if re.minimax_re.is_match(&new_line) {
            new_line = re
                .minimax_re
                .replace_all(&new_line, "[REDACTED-MINIMAX-KEY]")
                .to_string();
            warnings.push((line_num, "MiniMax API Key detected".to_string()));
        }

        if re.openai_re.is_match(&new_line) {
            new_line = re
                .openai_re
                .replace_all(&new_line, "[REDACTED-OPENAI-KEY]")
                .to_string();
            warnings.push((line_num, "OpenAI API Key detected".to_string()));
        }

        if re.gemini_re.is_match(&new_line) {
            new_line = re
                .gemini_re
                .replace_all(&new_line, "[REDACTED-GEMINI-KEY]")
                .to_string();
            warnings.push((line_num, "Gemini/Google API Key detected".to_string()));
        }

        if re.github_re.is_match(&new_line) {
            new_line = re
                .github_re
                .replace_all(&new_line, "[REDACTED-GITHUB-PAT]")
                .to_string();
            warnings.push((line_num, "GitHub PAT detected".to_string()));
        }

        if re.aws_id_re.is_match(&new_line) {
            new_line = re
                .aws_id_re
                .replace_all(&new_line, "[REDACTED-AWS-ACCESS-KEY-ID]")
                .to_string();
            warnings.push((line_num, "AWS Access Key ID detected".to_string()));
        }

        if re.aws_secret_re.is_match(&new_line) {
            new_line = re
                .aws_secret_re
                .replace_all(&new_line, "${1}[REDACTED-AWS-SECRET-KEY]${2}")
                .to_string();
            warnings.push((line_num, "AWS Secret Access Key detected".to_string()));
        }

        if re.stripe_re.is_match(&new_line) {
            new_line = re
                .stripe_re
                .replace_all(&new_line, "[REDACTED-STRIPE-KEY]")
                .to_string();
            warnings.push((line_num, "Stripe API Key detected".to_string()));
        }

        if re.minimax_re.is_match(&new_line) {
            new_line = re
                .minimax_re
                .replace_all(&new_line, "[REDACTED-MINIMAX-KEY]")
                .to_string();
            warnings.push((line_num, "MiniMax API Key detected".to_string()));
        }

        if re.generic_key_re.is_match(&new_line) {
            new_line = re
                .generic_key_re
                .replace_all(&new_line, "${1}${2}[REDACTED-SECRET]${3}")
                .to_string();
            warnings.push((
                line_num,
                "Potential generic credential/token detected".to_string(),
            ));
        }

        lines.push(new_line);
    }

    RedactResult {
        content: lines.join("\n"),
        warnings,
    }
}

/// Convenience fast-path: returns sanitized string with all secrets redacted.
pub fn sanitize_text(text: &str) -> String {
    redact_secrets(text).content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_openai_key() {
        let content = "let key = \"sk-proj-1234567890abcdef1234567890abcdef1234567890abcdef\";";
        let res = redact_secrets(content);
        assert_eq!(res.warnings.len(), 1);
        assert!(res.content.contains("[REDACTED-OPENAI-KEY]"));
    }

    #[test]
    fn test_redact_anthropic_key() {
        let content =
            "export ANTHROPIC_API_KEY=sk-ant-api03-1234567890abcdef1234567890abcdef1234567890";
        let res = redact_secrets(content);
        assert_eq!(res.warnings.len(), 1);
        assert!(res.content.contains("[REDACTED-ANTHROPIC-KEY]"));
    }

    #[test]
    fn test_redact_gemini_key() {
        let content = "GEMINI_API_KEY=\"AIzaSyD-1234567890abcdef1234567890abcde\"";
        let res = redact_secrets(content);
        assert_eq!(res.warnings.len(), 1);
        assert!(res.content.contains("[REDACTED-GEMINI-KEY]"));
    }

    #[test]
    fn test_redact_github_pat() {
        let content = "let token = \"ghp_1234567890abcdef1234567890abcdef1234\";";
        let res = redact_secrets(content);
        assert_eq!(res.warnings.len(), 1);
        assert!(res.content.contains("[REDACTED-GITHUB-PAT]"));
    }

    #[test]
    fn test_redact_generic_key() {
        let content = "db_password = \"my-super-secret-password-1234567890\";";
        let res = redact_secrets(content);
        assert_eq!(res.warnings.len(), 1);
        assert!(res.content.contains("[REDACTED-SECRET]"));
    }
}
