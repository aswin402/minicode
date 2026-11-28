//! Universal cross-language security & hygiene checks using zero-regex deterministic lexical
//! scanning, RFC 1918/loopback IP classification, and Shannon byte-entropy analysis.

use crate::detector::LoadedFile;
use crate::rules::make_diag;
use crate::types::{Diagnostic, DiagnosticCategory, Severity};
use std::net::IpAddr;

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

struct TokenPrefixSpec {
    prefix: &'static str,
    min_body_len: usize,
    allow_dash_underscore: bool,
    upper_alnum_only: bool,
}

const KNOWN_TOKEN_PREFIXES: &[TokenPrefixSpec] = &[
    TokenPrefixSpec {
        prefix: "sk-",
        min_body_len: 20,
        allow_dash_underscore: true,
        upper_alnum_only: false,
    },
    TokenPrefixSpec {
        prefix: "ghp_",
        min_body_len: 30,
        allow_dash_underscore: false,
        upper_alnum_only: false,
    },
    TokenPrefixSpec {
        prefix: "AKIA",
        min_body_len: 16,
        allow_dash_underscore: false,
        upper_alnum_only: true,
    },
    TokenPrefixSpec {
        prefix: "AIza",
        min_body_len: 35,
        allow_dash_underscore: true,
        upper_alnum_only: false,
    },
];

/// Computes Shannon byte entropy $H(X) = -\sum p_i \log_2 p_i$ in bits per byte.
fn shannon_entropy(s: &str) -> f64 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for &b in bytes {
        freq[b as usize] += 1;
    }
    let n = bytes.len() as f64;
    let mut h = 0.0;
    for &c in &freq {
        if c > 0 {
            let p = f64::from(c) / n;
            h -= p * p.log2();
        }
    }
    h
}

/// Deterministic zero-regex prefix scanner for known cloud/API token literals on a single line.
fn find_known_token_literal(line: &str) -> Option<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] >= 0x80 {
            i += line[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        for spec in KNOWN_TOKEN_PREFIXES {
            if line[i..].starts_with(spec.prefix) {
                let body_start = i + spec.prefix.len();
                let mut end = body_start;
                while end < bytes.len() {
                    let b = bytes[end];
                    let ok = if spec.upper_alnum_only {
                        b.is_ascii_uppercase() || b.is_ascii_digit()
                    } else if spec.allow_dash_underscore {
                        b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
                    } else {
                        b.is_ascii_alphanumeric()
                    };
                    if ok {
                        end += 1;
                    } else {
                        break;
                    }
                }
                if end - body_start >= spec.min_body_len {
                    return Some((i, &line[i..end]));
                }
            }
        }
        i += 1;
    }
    None
}

/// Deterministic zero-regex lexer for `IDENT [: Type] = "string_literal"` assignments.
fn parse_string_assignment(line: &str) -> Option<(&str, &str)> {
    let eq_pos = line.find('=')?;
    // Ignore comparison operators (`==`, `!=`, `<=`, `>=`, `=>`)
    let bytes = line.as_bytes();
    if eq_pos > 0 && matches!(bytes[eq_pos - 1], b'=' | b'!' | b'<' | b'>') {
        return None;
    }
    if bytes
        .get(eq_pos + 1)
        .is_some_and(|&b| matches!(b, b'=' | b'>'))
    {
        return None;
    }

    let lhs = line[..eq_pos].trim();
    // Strip optional type annotation (`IDENT: str = ...`)
    let lhs_no_type = lhs.split(':').next().unwrap_or(lhs).trim();
    // Extract the trailing identifier token (`const API_KEY`, `let SECRET_KEY`, `API_KEY`)
    let ident = lhs_no_type
        .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .find(|tok| !tok.is_empty())?;
    if ident.len() < 4 || ident.len() > 41 || !ident.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }

    let rhs = line[eq_pos + 1..].trim_start();
    let quote = rhs.bytes().next().filter(|&b| b == b'"' || b == b'\'')?;
    let rest = &rhs[1..];
    let close = rest.bytes().position(|b| b == quote)?;
    let val = &rest[..close];
    if (12..=120).contains(&val.len()) {
        Some((ident, val))
    } else {
        None
    }
}

/// Deterministic zero-regex extractor for quoted `http://...` URL literals and host classification.
fn find_quoted_http_url(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let q = bytes[i];
        if q == b'"' || q == b'\'' {
            let start = i + 1;
            if let Some(rel_end) = bytes[start..].iter().position(|&b| b == q) {
                let lit = &line[start..start + rel_end];
                if lit.starts_with("http://")
                    && lit.len() > 7
                    && !lit.bytes().any(|b| b.is_ascii_whitespace())
                {
                    return Some(lit);
                }
                i = start + rel_end + 1;
                continue;
            }
        }
        i += 1;
    }
    None
}

/// Classifies whether an `http://` URL targets a local/private host or well-known XML namespace.
fn is_benign_http_url(url: &str) -> bool {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_raw = if authority.starts_with('[') {
        authority
            .split(']')
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let host = host_raw.to_ascii_lowercase();

    if host == "localhost"
        || host == "example.com"
        || host == "www.w3.org"
        || host.starts_with("schemas.")
        || host.ends_with(".local")
    {
        return true;
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_unspecified() || v4.is_link_local()
            }
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        };
    }
    false
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
            if let Some((start_idx, matched_tok)) = find_known_token_literal(line) {
                let lower_val = matched_tok.to_ascii_lowercase();
                if !PLACEHOLDER_FRAGMENTS.iter().any(|p| lower_val.contains(p)) {
                    out.push(make_diag(
                        "sec/hardcoded-secret",
                        Severity::Critical,
                        DiagnosticCategory::Security,
                        &file.rel_path,
                        line_no,
                        start_idx + 1,
                        "Hardcoded cloud/API credential literal detected in source code",
                        "Move this key to environment variables or a secret manager and rotate it.",
                        &file.content,
                    ));
                    continue;
                }
            }

            // 3. Hardcoded SECRET / PASSWORD assignments with `SAFE_KEY_EXCLUSIONS` + entropy validation
            if let Some((ident, val)) = parse_string_assignment(line) {
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
                    || val.contains(' ')
                    || shannon_entropy(val) < 2.2;

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
            if let Some(url) = find_quoted_http_url(line) {
                if !is_benign_http_url(url) {
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
