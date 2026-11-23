//! Universal cross-language security & hygiene checks (zero false-positive secret scanning
//! inspired by `py_doctor`'s `SAFE_KEY_EXCLUSIONS`, JWT `alg: none`, conflict markers, HTTP URLs).

use crate::detector::LoadedFile;
use crate::rules::make_diag;
use crate::types::{Diagnostic, DiagnosticCategory, Severity};
use regex::Regex;
use std::sync::OnceLock;

const SECRET_IDENT_TOKENS: &[&str] = &[
    "SECRET_KEY",
    "API_KEY",
    "PRIVATE_KEY",
    "JWT_SECRET",
    "ACCESS_KEY",
    "AUTH_KEY",
    "ENCRYPTION_KEY",
    "SIGNING_KEY",
    "CLIENT_SECRET",
    "DB_PASSWORD",
    "DATABASE_PASSWORD",
    "PASSWORD",
    "PASSWD",
    "AUTH_TOKEN",
    "BEARER_TOKEN",
];

const SAFE_KEY_EXCLUSIONS: &[&str] = &[
    "PRIMARY_KEY",
    "FOREIGN_KEY",
    "CACHE_KEY",
    "SORT_KEY",
    "PUBLIC_KEY",
    "PARTITION_KEY",
    "ROUTING_KEY",
    "DICT_KEY",
    "ITEM_KEY",
    "IDEMPOTENCY_KEY",
    "SEARCH_KEY",
    "ENV_KEY",
    "KEY_NAME",
    "DEDUP_KEY",
    "TEST_PROVIDER_ENV_KEYS",
];

const PLACEHOLDER_FRAGMENTS: &[&str] = &[
    "your_",
    "your-",
    "example",
    "placeholder",
    "changeme",
    "replace_me",
    "dummy",
    "sample",
    "test_",
    "mock_",
    "fake_",
    "xxxx",
    "****",
    "<your",
    "${",
    "process.env",
    "os.environ",
    "os.getenv",
    "std::env",
    "env!",
];

fn secret_assign_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)\b([A-Z][A-Z0-9_]{3,40})\b\s*(?::\s*[^=]+)?=\s*["']([^"'\n]{12,120})["']"#,
        )
        .unwrap_or_else(|_| Regex::new("a^").unwrap_or_else(|_| unreachable!()))
    })
}

fn known_token_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?:sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{30,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z\-_]{35})"#)
            .unwrap_or_else(|_| Regex::new("a^").unwrap_or_else(|_| unreachable!()))
    })
}

fn http_url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"["'](http://[a-zA-Z0-9.\-]+(?::\d+)?(?:/[^"'\s]*)?)["']"#)
            .unwrap_or_else(|_| Regex::new("a^").unwrap_or_else(|_| unreachable!()))
    })
}

fn is_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("--")
}

/// Runs universal checks applicable to every source file.
pub fn check_universal(file: &LoadedFile, run_security: bool, out: &mut Vec<Diagnostic>) {
    let secret_re = secret_assign_regex();
    let token_re = known_token_prefix_regex();
    let http_re = http_url_regex();
    let mut in_rust_test_mod = false;

    for (idx, line) in file.content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();

        if file.ext == "rs" && trimmed.contains("#[cfg(test)]") {
            in_rust_test_mod = true;
        }
        let skip_security_in_tests = file.is_test || in_rust_test_mod;

        // 1. Unresolved Git merge conflict markers
        if trimmed.starts_with("<<<<<<< ")
            || trimmed == "<<<<<<<"
            || trimmed.starts_with(">>>>>>> ")
            || trimmed == ">>>>>>>"
        {
            out.push(make_diag(
                "core/merge-conflict-marker",
                Severity::Critical,
                DiagnosticCategory::Correctness,
                &file.rel_path,
                line_no,
                1,
                "Unresolved Git merge conflict marker detected",
                "Resolve both conflict branches and remove `<<<<<<<` / `=======` / `>>>>>>>` lines.",
                &file.content,
            ));
        }

        if !run_security || is_comment_line(trimmed) {
            continue;
        }

        // 2. Hardcoded provider API keys / token prefixes (skip test suites)
        if !skip_security_in_tests {
            if let Some(m) = token_re.find(line) {
                let lower_val = m.as_str().to_ascii_lowercase();
                if !PLACEHOLDER_FRAGMENTS.iter().any(|p| lower_val.contains(p)) {
                    out.push(make_diag(
                        "sec/hardcoded-secret",
                        Severity::Critical,
                        DiagnosticCategory::Security,
                        &file.rel_path,
                        line_no,
                        m.start() + 1,
                        "Hardcoded cloud/API credential literal detected in source code",
                        "Move this key to environment variables or a secret manager and rotate it.",
                        &file.content,
                    ));
                    continue;
                }
            }

            // 3. Hardcoded SECRET / PASSWORD assignments with `SAFE_KEY_EXCLUSIONS`
            if let Some(caps) = secret_re.captures(line) {
                let ident = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let val = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let ident_upper = ident.to_ascii_uppercase();
                let val_lower = val.to_ascii_lowercase();

                let is_secret_ident = SECRET_IDENT_TOKENS
                    .iter()
                    .any(|tok| ident_upper == *tok || ident_upper.ends_with(tok));
                let is_safe_ident = SAFE_KEY_EXCLUSIONS
                    .iter()
                    .any(|safe| ident_upper.contains(safe));
                let is_placeholder = PLACEHOLDER_FRAGMENTS.iter().any(|p| val_lower.contains(p))
                    || val.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                    || val.contains(' ');

                if is_secret_ident && !is_safe_ident && !is_placeholder {
                    out.push(make_diag(
                        "sec/hardcoded-secret",
                        Severity::Critical,
                        DiagnosticCategory::Security,
                        &file.rel_path,
                        line_no,
                        1,
                        format!("Hardcoded secret assigned to `{ident}` — load from environment instead"),
                        format!("Replace the literal with an environment lookup for `{ident_upper}`."),
                        &file.content,
                    ));
                }
            }
        }

        // 4. JWT `none` algorithm check (inspired by `hono_doctor`)
        let lower = trimmed.to_ascii_lowercase();
        if !skip_security_in_tests
            && (lower.contains("algorithm") || lower.contains("alg:") || lower.contains("\"alg\""))
            && (lower.contains("'none'") || lower.contains("\"none\""))
        {
            out.push(make_diag(
                "sec/jwt-none-algorithm",
                Severity::Critical,
                DiagnosticCategory::Security,
                &file.rel_path,
                line_no,
                1,
                "JWT configured with insecure `none` algorithm",
                "Enforce a cryptographic signing algorithm such as HS256, RS256, or EdDSA.",
                &file.content,
            ));
        }

        // 5. Cleartext `http://` URLs targeting external hosts
        if !skip_security_in_tests {
            if let Some(caps) = http_re.captures(line) {
                let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let u_lower = url.to_ascii_lowercase();
                let benign = u_lower.starts_with("http://localhost")
                    || u_lower.starts_with("http://127.0.0.1")
                    || u_lower.starts_with("http://0.0.0.0")
                    || u_lower.starts_with("http://[::1]")
                    || u_lower.starts_with("http://www.w3.org/")
                    || u_lower.starts_with("http://schemas.")
                    || u_lower.starts_with("http://example.com")
                    || u_lower.starts_with("http://192.168.")
                    || u_lower.starts_with("http://10.");
                if !benign {
                    out.push(make_diag(
                        "sec/insecure-http-url",
                        Severity::Warning,
                        DiagnosticCategory::Security,
                        &file.rel_path,
                        line_no,
                        1,
                        format!("Cleartext HTTP endpoint `{url}` — prefer HTTPS in production"),
                        "Switch the URL scheme to `https://` unless strictly local development.",
                        &file.content,
                    ));
                }
            }
        }
    }
}
