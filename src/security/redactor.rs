/// Result of a secret redaction pass over text or code.
#[derive(Debug, Clone)]
pub struct RedactResult {
    pub content: String,
    pub warnings: Vec<(usize, String)>,
}

/// Scans content for secrets and replaces them with typed redaction markers by delegating
/// to the unified [`crate::sandbox::redact::SecretRedactor`] engine (prefix patterns +
/// Shannon byte-entropy detection + environment secret harvesting).
pub fn redact_secrets(content: &str) -> RedactResult {
    let (redacted_content, warnings) =
        crate::sandbox::redact::SecretRedactor::global().redact_detailed(content);
    RedactResult {
        content: redacted_content,
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
