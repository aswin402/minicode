use crate::constants::{BLOCKED_PREFIXES, REDACTED_PLACEHOLDER, SECRET_PATTERNS};
use std::collections::HashSet;
use std::sync::OnceLock;

static GLOBAL_REDACTOR: OnceLock<SecretRedactor> = OnceLock::new();

/// Minimum Shannon entropy (bits per byte) for unstructured secret value detection.
const SECRET_SHANNON_ENTROPY_MIN_BITS: f64 = 4.10;
/// Minimum candidate value length for entropy-based secret redaction.
const SECRET_ENTROPY_MIN_LEN: usize = 20;

#[derive(Clone, Copy)]
enum TokenBodyClass {
    /// `[A-Za-z0-9]`
    Alnum,
    /// `[A-Za-z0-9_-]`
    Base64Url,
    /// `[A-Za-z0-9_]`
    Word,
    /// `[0-9A-Z]`
    UpperAlnum,
}

impl TokenBodyClass {
    #[inline]
    fn accepts(self, b: u8) -> bool {
        match self {
            Self::Alnum => b.is_ascii_alphanumeric(),
            Self::Base64Url => b.is_ascii_alphanumeric() || b == b'-' || b == b'_',
            Self::Word => b.is_ascii_alphanumeric() || b == b'_',
            Self::UpperAlnum => b.is_ascii_uppercase() || b.is_ascii_digit(),
        }
    }
}

/// Declarative specification for prefixed provider credentials (zero regex).
struct PrefixTokenRule {
    prefix: &'static str,
    min_body_len: usize,
    exact_body_len: Option<usize>,
    body_class: TokenBodyClass,
    replacement: &'static str,
    typed_replacement: &'static str,
    warning_msg: &'static str,
}

/// Priority-ordered prefix table (more specific prefixes like `sk-ant-`/`sk-proj-` precede `sk-`).
const PREFIX_TOKEN_RULES: &[PrefixTokenRule] = &[
    PrefixTokenRule {
        prefix: "sk-ant-",
        min_body_len: 20,
        exact_body_len: None,
        body_class: TokenBodyClass::Base64Url,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-ANTHROPIC-KEY]",
        warning_msg: "Anthropic API Key detected",
    },
    PrefixTokenRule {
        prefix: "sk-cp-",
        min_body_len: 32,
        exact_body_len: None,
        body_class: TokenBodyClass::Base64Url,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-MINIMAX-KEY]",
        warning_msg: "MiniMax API Key detected",
    },
    PrefixTokenRule {
        prefix: "sk-proj-",
        min_body_len: 20,
        exact_body_len: None,
        body_class: TokenBodyClass::Base64Url,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-OPENAI-KEY]",
        warning_msg: "OpenAI API Key detected",
    },
    PrefixTokenRule {
        prefix: "sk_live_",
        min_body_len: 24,
        exact_body_len: None,
        body_class: TokenBodyClass::Alnum,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-STRIPE-KEY]",
        warning_msg: "Stripe API Key detected",
    },
    PrefixTokenRule {
        prefix: "sk_test_",
        min_body_len: 24,
        exact_body_len: None,
        body_class: TokenBodyClass::Alnum,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-STRIPE-KEY]",
        warning_msg: "Stripe API Key detected",
    },
    PrefixTokenRule {
        prefix: "rk_live_",
        min_body_len: 24,
        exact_body_len: None,
        body_class: TokenBodyClass::Alnum,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-STRIPE-KEY]",
        warning_msg: "Stripe API Key detected",
    },
    PrefixTokenRule {
        prefix: "rk_test_",
        min_body_len: 24,
        exact_body_len: None,
        body_class: TokenBodyClass::Alnum,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-STRIPE-KEY]",
        warning_msg: "Stripe API Key detected",
    },
    PrefixTokenRule {
        prefix: "sk-",
        min_body_len: 20,
        exact_body_len: None,
        body_class: TokenBodyClass::Alnum,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-OPENAI-KEY]",
        warning_msg: "OpenAI API Key detected",
    },
    PrefixTokenRule {
        prefix: "github_pat_",
        min_body_len: 22,
        exact_body_len: None,
        body_class: TokenBodyClass::Word,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-GITHUB-PAT]",
        warning_msg: "GitHub PAT detected",
    },
    PrefixTokenRule {
        prefix: "ghp_",
        min_body_len: 36,
        exact_body_len: None,
        body_class: TokenBodyClass::Word,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-GITHUB-PAT]",
        warning_msg: "GitHub PAT detected",
    },
    PrefixTokenRule {
        prefix: "ghs_",
        min_body_len: 36,
        exact_body_len: None,
        body_class: TokenBodyClass::Word,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-GITHUB-PAT]",
        warning_msg: "GitHub PAT detected",
    },
    PrefixTokenRule {
        prefix: "AKIA",
        min_body_len: 16,
        exact_body_len: Some(16),
        body_class: TokenBodyClass::UpperAlnum,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-AWS-ACCESS-KEY-ID]",
        warning_msg: "AWS Access Key ID detected",
    },
    PrefixTokenRule {
        prefix: "AIza",
        min_body_len: 35,
        exact_body_len: Some(35),
        body_class: TokenBodyClass::Base64Url,
        replacement: REDACTED_PLACEHOLDER,
        typed_replacement: "[REDACTED-GEMINI-KEY]",
        warning_msg: "Gemini/Google API Key detected",
    },
];

struct ScanMatch {
    end: usize,
    replacement: &'static str,
    typed_replacement: &'static str,
    warning_msg: &'static str,
}

/// Zero-leak, zero-regex secret redaction engine.
///
/// Combines:
/// 1. Multi-line PEM private key block FSM
/// 2. Single-pass O(n) lexical scanner (provider prefix table, Slack/JWT/Bearer structural validators,
///    and credential assignment lexer)
/// 3. Lazy-allocating Shannon byte-entropy classifier for novel/unlisted tokens
/// 4. Exact-match host environment secret harvesting
pub struct SecretRedactor {
    /// Exact secret values harvested from the host environment at init time
    env_secrets: HashSet<String>,
}

impl SecretRedactor {
    /// Returns the process-global redactor instance, initializing on first call.
    pub fn global() -> &'static SecretRedactor {
        GLOBAL_REDACTOR.get_or_init(Self::new)
    }

    /// Constructs a new redactor with harvested env secrets.
    fn new() -> Self {
        let env_secrets = Self::harvest_env_secrets();

        tracing::info!(
            prefix_rule_count = PREFIX_TOKEN_RULES.len(),
            env_secret_count = env_secrets.len(),
            "SecretRedactor initialized (zero-regex deterministic FSM)"
        );

        Self { env_secrets }
    }

    /// Creates a redactor with only deterministic lexical/entropy rules (no env harvesting).
    #[cfg(test)]
    pub fn patterns_only() -> Self {
        Self {
            env_secrets: HashSet::new(),
        }
    }

    /// Creates a redactor with custom exact-match secrets for testing.
    #[cfg(test)]
    pub fn with_secrets(secrets: HashSet<String>) -> Self {
        Self {
            env_secrets: secrets,
        }
    }

    /// Apply all redaction rules to the input string (zero-alloc fast path when clean).
    pub fn redact(&self, input: &str) -> String {
        if input.is_empty() {
            return String::new();
        }

        let mut output = std::borrow::Cow::Borrowed(input);

        // Phase 1: Multi-line PEM private key block FSM
        if output.contains("-----BEGIN") {
            if let Some(redacted) = Self::redact_pem_blocks(&output) {
                tracing::debug!("Redacted PEM private key block");
                output = std::borrow::Cow::Owned(redacted);
            }
        }

        // Phase 2: Single-pass O(n) lexical scanner (prefixed tokens, Slack, Bearer, JWT, assignments)
        if let Some((redacted, _)) = Self::scan_and_redact_text(&output, false) {
            output = std::borrow::Cow::Owned(redacted);
        }

        // Phase 3: Literal-gated Shannon byte-entropy pass for novel/unlisted provider tokens
        if output.len() >= SECRET_ENTROPY_MIN_LEN && (output.contains('=') || output.contains(':'))
        {
            if let Some(entropy_redacted) =
                Self::redact_high_entropy_assignments(&output, REDACTED_PLACEHOLDER)
            {
                tracing::debug!("Redacted high-entropy credential assignment");
                output = std::borrow::Cow::Owned(entropy_redacted);
            }
        }

        // Phase 4: Exact-match env secret replacement
        for secret_val in &self.env_secrets {
            if secret_val.len() >= 8 && output.contains(secret_val.as_str()) {
                output = std::borrow::Cow::Owned(
                    output.replace(secret_val.as_str(), REDACTED_PLACEHOLDER),
                );
                tracing::debug!("Redacted exact-match environment secret");
            }
        }

        output.into_owned()
    }

    /// Scans content and returns both typed-marker redacted content and `(1-indexed line, warning)` entries.
    pub fn redact_detailed(&self, content: &str) -> (String, Vec<(usize, String)>) {
        if content.is_empty() {
            return (String::new(), Vec::new());
        }

        let mut working = std::borrow::Cow::Borrowed(content);
        let mut warnings = Vec::new();

        // Phase 1: Multi-line PEM private key FSM
        if working.contains("-----BEGIN") {
            if let Some(redacted) = Self::redact_pem_blocks(&working) {
                warnings.push((1, "PEM Private Key detected".to_string()));
                working = std::borrow::Cow::Owned(redacted);
            }
        }

        let maybe_has_entropy = working.len() >= SECRET_ENTROPY_MIN_LEN
            && (working.contains('=') || working.contains(':'));
        let has_env_hit = self
            .env_secrets
            .iter()
            .any(|s| s.len() >= 8 && working.contains(s.as_str()));

        // Phase 2: Per-line zero-regex lexical scan + Shannon entropy + env secret pass (lazy alloc)
        let mut out_lines: Option<Vec<String>> = if warnings.is_empty() {
            None
        } else {
            Some(Vec::new())
        };
        let mut raw_lines_seen: Vec<&str> = Vec::new();

        for (idx, line) in working.lines().enumerate() {
            let line_num = idx + 1;
            let mut current = std::borrow::Cow::Borrowed(line);

            if let Some((redacted, line_warnings)) = Self::scan_and_redact_text(&current, true) {
                current = std::borrow::Cow::Owned(redacted);
                for w in line_warnings {
                    warnings.push((line_num, w.to_string()));
                }
            }

            if maybe_has_entropy
                && current.len() >= SECRET_ENTROPY_MIN_LEN
                && (current.contains('=') || current.contains(':'))
            {
                if let Some(entropy_redacted) =
                    Self::redact_high_entropy_assignments(&current, "[REDACTED-SECRET]")
                {
                    current = std::borrow::Cow::Owned(entropy_redacted);
                    warnings.push((
                        line_num,
                        "High-entropy credential/secret value detected".to_string(),
                    ));
                }
            }

            if has_env_hit {
                for secret_val in &self.env_secrets {
                    if secret_val.len() >= 8 && current.contains(secret_val.as_str()) {
                        current = std::borrow::Cow::Owned(
                            current.replace(secret_val.as_str(), "[REDACTED-SECRET]"),
                        );
                        warnings.push((line_num, "Environment secret value detected".to_string()));
                    }
                }
            }

            match (&mut out_lines, current) {
                (Some(vec), cow) => vec.push(cow.into_owned()),
                (None, std::borrow::Cow::Owned(mutated)) => {
                    let mut vec = Vec::with_capacity(raw_lines_seen.len() + 8);
                    for prev in &raw_lines_seen {
                        vec.push((*prev).to_string());
                    }
                    vec.push(mutated);
                    out_lines = Some(vec);
                }
                (None, std::borrow::Cow::Borrowed(orig)) => {
                    raw_lines_seen.push(orig);
                }
            }
        }

        let final_content = match out_lines {
            None => content.to_string(),
            Some(vec) => {
                let mut joined = vec.join("\n");
                if content.ends_with('\n') && !joined.ends_with('\n') {
                    joined.push('\n');
                }
                joined
            }
        };

        (final_content, warnings)
    }

    /// Deterministic multi-line PEM private key block FSM (`-----BEGIN ... PRIVATE KEY-----` to `-----END ... PRIVATE KEY-----`).
    fn redact_pem_blocks(input: &str) -> Option<String> {
        let mut out: Option<String> = None;
        let mut cursor = 0usize;

        while let Some(rel_begin) = input[cursor..].find("-----BEGIN") {
            let start = cursor + rel_begin;
            let after_begin = &input[start + 10..];
            let Some(header_end_rel) = after_begin.find("-----") else {
                break;
            };
            let header_mid = after_begin[..header_end_rel].trim();
            if !header_mid.ends_with("PRIVATE KEY") {
                cursor = start + 10;
                continue;
            }
            let body_start = start + 10 + header_end_rel + 5;
            let end_marker = format!("-----END {}-----", header_mid);
            let Some(end_rel) = input[body_start..].find(&end_marker) else {
                cursor = body_start;
                continue;
            };
            let block_end = body_start + end_rel + end_marker.len();
            let buf = out.get_or_insert_with(|| String::with_capacity(input.len()));
            buf.push_str(&input[cursor..start]);
            buf.push_str("[REDACTED: PRIVATE KEY]");
            cursor = block_end;
        }

        out.map(|mut buf| {
            buf.push_str(&input[cursor..]);
            buf
        })
    }

    /// Single-pass O(n) lexical scanner replacing all 16 legacy regex rules with zero allocations on clean text.
    fn scan_and_redact_text(input: &str, typed: bool) -> Option<(String, Vec<&'static str>)> {
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut out: Option<String> = None;
        let mut warnings: Vec<&'static str> = Vec::new();
        let mut last_copy = 0usize;
        let mut i = 0usize;

        while i < len {
            if bytes[i] >= 0x80 {
                i += input[i..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
            let matched = Self::try_match_at(input, bytes, i);
            if let Some(m) = matched {
                let buf = out.get_or_insert_with(|| String::with_capacity(len));
                buf.push_str(&input[last_copy..i]);
                buf.push_str(if typed {
                    m.typed_replacement
                } else {
                    m.replacement
                });
                warnings.push(m.warning_msg);
                i = m.end;
                last_copy = i;
            } else {
                i += 1;
            }
        }

        out.map(|mut buf| {
            buf.push_str(&input[last_copy..]);
            (buf, warnings)
        })
    }

    /// Attempts deterministic token/credential grammars anchored at byte index `i`.
    #[inline]
    fn try_match_at(input: &str, bytes: &[u8], i: usize) -> Option<ScanMatch> {
        let rest = &input[i..];
        let b = bytes[i];

        // 1. Prefixed cloud/API provider tokens (dispatch on leading byte)
        if matches!(b, b's' | b'r' | b'g' | b'A') {
            for rule in PREFIX_TOKEN_RULES {
                if rest.starts_with(rule.prefix) {
                    let body_start = i + rule.prefix.len();
                    let mut end = body_start;
                    let max_end = rule
                        .exact_body_len
                        .map_or(bytes.len(), |n| (body_start + n).min(bytes.len()));
                    while end < max_end && rule.body_class.accepts(bytes[end]) {
                        end += 1;
                    }
                    let body_len = end - body_start;
                    let ok = match rule.exact_body_len {
                        Some(exact) => body_len == exact,
                        None => body_len >= rule.min_body_len,
                    };
                    if ok {
                        return Some(ScanMatch {
                            end,
                            replacement: rule.replacement,
                            typed_replacement: rule.typed_replacement,
                            warning_msg: rule.warning_msg,
                        });
                    }
                }
            }
        }

        // 2. Slack tokens: `xox[bpras]-[0-9]{10,}-[A-Za-z0-9]+`
        if b == b'x'
            && rest.len() >= 18
            && rest.starts_with("xox")
            && matches!(bytes.get(i + 3), Some(b'b' | b'p' | b'r' | b'a' | b's'))
            && bytes.get(i + 4) == Some(&b'-')
        {
            let mut p = i + 5;
            let digits_start = p;
            while p < bytes.len() && bytes[p].is_ascii_digit() {
                p += 1;
            }
            if p - digits_start >= 10 && bytes.get(p) == Some(&b'-') {
                p += 1;
                let tail_start = p;
                while p < bytes.len() && bytes[p].is_ascii_alphanumeric() {
                    p += 1;
                }
                if p > tail_start {
                    return Some(ScanMatch {
                        end: p,
                        replacement: REDACTED_PLACEHOLDER,
                        typed_replacement: "[REDACTED-SLACK-TOKEN]",
                        warning_msg: "Slack Token detected",
                    });
                }
            }
        }

        // 3. Bearer token: `Bearer\s+[A-Za-z0-9\-._~+/]{16,}=*`
        if b == b'B' && rest.starts_with("Bearer") {
            let mut p = i + 6;
            let ws_start = p;
            while p < bytes.len() && matches!(bytes[p], b' ' | b'\t') {
                p += 1;
            }
            if p > ws_start {
                let tok_start = p;
                while p < bytes.len()
                    && (bytes[p].is_ascii_alphanumeric()
                        || matches!(bytes[p], b'-' | b'.' | b'_' | b'~' | b'+' | b'/'))
                {
                    p += 1;
                }
                if p - tok_start >= 16 {
                    while p < bytes.len() && bytes[p] == b'=' {
                        p += 1;
                    }
                    return Some(ScanMatch {
                        end: p,
                        replacement: "Bearer [REDACTED]",
                        typed_replacement: "Bearer [REDACTED]",
                        warning_msg: "Bearer Token detected",
                    });
                }
            }
        }

        // 4. Structural 3-part JWT: `eyJ<b64>.eyJ<b64>.<b64>`
        if b == b'e' && rest.starts_with("eyJ") {
            let is_b64url = |c: u8| c.is_ascii_alphanumeric() || c == b'-' || c == b'_';
            let mut p = i + 3;
            while p < bytes.len() && is_b64url(bytes[p]) {
                p += 1;
            }
            if p > i + 3 && input[p..].starts_with(".eyJ") {
                p += 4;
                let s2_start = p;
                while p < bytes.len() && is_b64url(bytes[p]) {
                    p += 1;
                }
                if p > s2_start && bytes.get(p) == Some(&b'.') {
                    p += 1;
                    let s3_start = p;
                    while p < bytes.len() && is_b64url(bytes[p]) {
                        p += 1;
                    }
                    if p > s3_start {
                        return Some(ScanMatch {
                            end: p,
                            replacement: REDACTED_PLACEHOLDER,
                            typed_replacement: "[REDACTED-JWT]",
                            warning_msg: "JWT Token detected",
                        });
                    }
                }
            }
        }

        // 5. Credential key=value lexical assignments (only at identifier boundary)
        if b.is_ascii_alphabetic()
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_')
        {
            if let Some(m) = Self::try_match_credential_assignment(input, bytes, i) {
                return Some(m);
            }
        }

        None
    }

    /// Matches credential assignment forms (`aws_secret_access_key=...`, `password=...`, `api_key=...`,
    /// hex `secret=...`) at an identifier boundary without regex backtracking.
    fn try_match_credential_assignment(
        input: &str,
        bytes: &[u8],
        start: usize,
    ) -> Option<ScanMatch> {
        // Guard: ignore scoped package keys (`@org/pkg`) or path segments immediately preceding
        if start > 0 && matches!(bytes[start - 1], b'/' | b'@') {
            return None;
        }
        let mut p = start;
        while p < bytes.len()
            && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'_' || bytes[p] == b'-')
        {
            p += 1;
        }
        let key_len = p - start;
        if !(3..=40).contains(&key_len) {
            return None;
        }
        let key_lower = input[start..p].to_ascii_lowercase();
        let after_key_char = bytes.get(p).copied();
        if matches!(after_key_char, Some(b'/' | b'@')) {
            return None;
        }

        // Classify key family
        let is_aws_secret = key_lower.starts_with("aws")
            && key_lower.contains("secret")
            && key_lower.ends_with("key");
        let is_conn_pwd = matches!(key_lower.as_str(), "password" | "pwd");
        let is_generic_kv = matches!(
            key_lower.as_str(),
            "apikey"
                | "api_key"
                | "api-key"
                | "apisecret"
                | "api_secret"
                | "api-secret"
                | "apitoken"
                | "api_token"
                | "api-token"
                | "accesstoken"
                | "access_token"
                | "access-token"
                | "authtoken"
                | "auth_token"
                | "auth-token"
                | "secretkey"
                | "secret_key"
                | "secret-key"
                | "clientsecret"
                | "client_secret"
                | "client-secret"
                | "clientkey"
                | "client_key"
                | "client-key"
                | "dbpassword"
                | "db_password"
                | "db-password"
        );
        let is_hex_key = matches!(key_lower.as_str(), "secret" | "token" | "credential");

        if !is_aws_secret && !is_conn_pwd && !is_generic_kv && !is_hex_key {
            return None;
        }

        // Skip optional whitespace, require `=` or `:` (only `=` for connection password)
        while p < bytes.len() && matches!(bytes[p], b' ' | b'\t') {
            p += 1;
        }
        let sep = *bytes.get(p)?;
        if is_conn_pwd {
            if sep != b'=' {
                return None;
            }
        } else if sep != b'=' && sep != b':' {
            return None;
        }
        p += 1;
        while p < bytes.len() && matches!(bytes[p], b' ' | b'\t') {
            p += 1;
        }
        if p >= bytes.len() {
            return None;
        }

        // Optional opening quote
        let quote = if !is_conn_pwd && matches!(bytes[p], b'"' | b'\'') {
            let q = bytes[p];
            p += 1;
            Some(q)
        } else {
            None
        };
        let val_start = p;

        if is_aws_secret {
            while p < bytes.len()
                && (bytes[p].is_ascii_alphanumeric() || matches!(bytes[p], b'/' | b'+' | b'='))
            {
                p += 1;
            }
            if p - val_start == 40 {
                if let Some(q) = quote {
                    if bytes.get(p) == Some(&q) {
                        p += 1;
                    }
                }
                return Some(ScanMatch {
                    end: p,
                    replacement: REDACTED_PLACEHOLDER,
                    typed_replacement: "[REDACTED-AWS-SECRET-KEY]",
                    warning_msg: "AWS Secret Access Key detected",
                });
            }
            return None;
        }

        if is_conn_pwd {
            if matches!(
                bytes[val_start],
                b';' | b'&' | b'[' | b' ' | b'\t' | b'\n' | b'\r'
            ) {
                return None;
            }
            while p < bytes.len()
                && !bytes[p].is_ascii_whitespace()
                && !matches!(bytes[p], b';' | b'&')
            {
                p += 1;
            }
            if p - val_start >= 8 {
                return Some(ScanMatch {
                    end: p,
                    replacement: REDACTED_PLACEHOLDER,
                    typed_replacement: "[REDACTED-SECRET]",
                    warning_msg: "Connection password detected",
                });
            }
            return None;
        }

        if is_generic_kv {
            while p < bytes.len()
                && (bytes[p].is_ascii_alphanumeric()
                    || matches!(bytes[p], b'-' | b'.' | b'_' | b'~' | b'+' | b'/'))
            {
                p += 1;
            }
            if p - val_start >= 16 {
                if let Some(q) = quote {
                    if bytes.get(p) == Some(&q) {
                        p += 1;
                    }
                }
                return Some(ScanMatch {
                    end: p,
                    replacement: REDACTED_PLACEHOLDER,
                    typed_replacement: "[REDACTED-SECRET]",
                    warning_msg: "Potential generic credential/token detected",
                });
            }
            return None;
        }

        if is_hex_key {
            while p < bytes.len() && bytes[p].is_ascii_hexdigit() {
                p += 1;
            }
            if p - val_start >= 32 {
                if let Some(q) = quote {
                    if bytes.get(p) == Some(&q) {
                        p += 1;
                    }
                }
                return Some(ScanMatch {
                    end: p,
                    replacement: REDACTED_PLACEHOLDER,
                    typed_replacement: "[REDACTED-SECRET]",
                    warning_msg: "Hex secret detected",
                });
            }
        }

        None
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
    /// Lazily allocates an output buffer ONLY if a line actually matches, keeping clean code scans zero-alloc.
    fn redact_high_entropy_assignments(input: &str, marker: &str) -> Option<String> {
        let mut out: Option<String> = None;
        let mut byte_offset = 0usize;

        for line in input.split('\n') {
            let mut matched_redacted = false;
            if line.len() >= SECRET_ENTROPY_MIN_LEN {
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
                                    let buf = match &mut out {
                                        Some(existing) => {
                                            existing.push('\n');
                                            existing
                                        }
                                        None => {
                                            let mut s = String::with_capacity(input.len());
                                            s.push_str(&input[..byte_offset]);
                                            out.insert(s)
                                        }
                                    };
                                    buf.push_str(&line[..=sep_pos]);
                                    buf.push_str(&rhs[..ws_len]);
                                    buf.push(quote);
                                    buf.push_str(marker);
                                    buf.push(quote);
                                    buf.push_str(&inner[end_q + quote.len_utf8()..]);
                                    matched_redacted = true;
                                }
                            }
                        } else {
                            let val_end = rhs_trimmed
                                .find(|c: char| {
                                    c.is_whitespace() || matches!(c, ';' | ',' | ')' | '}')
                                })
                                .unwrap_or(rhs_trimmed.len());
                            let candidate = &rhs_trimmed[..val_end];
                            if Self::is_high_entropy_secret_value(candidate) {
                                let buf = match &mut out {
                                    Some(existing) => {
                                        existing.push('\n');
                                        existing
                                    }
                                    None => {
                                        let mut s = String::with_capacity(input.len());
                                        s.push_str(&input[..byte_offset]);
                                        out.insert(s)
                                    }
                                };
                                buf.push_str(&line[..=sep_pos]);
                                buf.push_str(&rhs[..ws_len]);
                                buf.push_str(marker);
                                buf.push_str(&rhs_trimmed[val_end..]);
                                matched_redacted = true;
                            }
                        }
                    }
                }
            }

            if !matched_redacted {
                if let Some(ref mut existing) = out {
                    existing.push('\n');
                    existing.push_str(line);
                }
            }
            byte_offset = byte_offset.saturating_add(line.len() + 1);
        }

        out
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

    #[test]
    fn test_zero_regex_fsm_clean_fast_path_and_multi_line_entropy_preservation() {
        let r = SecretRedactor::patterns_only();

        let clean_rust =
            "fn main() {\n    let x: usize = 42;\n    let label = \"ordinary_value\";\n}\n";
        let (detailed, warnings) = r.redact_detailed(clean_rust);
        assert!(warnings.is_empty());
        assert_eq!(detailed, clean_rust);
        assert_eq!(r.redact(clean_rust), clean_rust);

        let mixed_entropy =
            "header = 1\nwebhook_signing_key = \"v9K2mP8xQ4rT7wZ1bN6jL3hF5dS0aG8eY2uI!\"\nfooter = 2\n";
        let (redacted_detailed, warn_list) = r.redact_detailed(mixed_entropy);
        assert_eq!(warn_list.len(), 1);
        assert_eq!(warn_list[0].0, 2);
        assert_eq!(
            redacted_detailed,
            "header = 1\nwebhook_signing_key = \"[REDACTED-SECRET]\"\nfooter = 2\n"
        );

        let mixed_dfa =
            "header = 1\napi_secret = \"v9K2mP8xQ4rT7wZ1bN6jL3hF5dS0aG8eY2uI\"\nfooter = 2\n";
        let (redacted_dfa, dfa_warns) = r.redact_detailed(mixed_dfa);
        assert_eq!(dfa_warns.len(), 1);
        assert_eq!(dfa_warns[0].0, 2);
        assert!(redacted_dfa.contains("[REDACTED-SECRET]"));
        assert!(!redacted_dfa.contains("v9K2mP8xQ4rT7wZ1"));
        assert!(r.redact(mixed_dfa).contains(REDACTED_PLACEHOLDER));
    }
}
