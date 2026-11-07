//! Information-theoretic Shannon entropy analyzer, binary/passive-image magic classifier, and Unicode script-mixing automaton.
//!
//! Implements pure algorithmic detection (zero regular expressions, zero ad-hoc `if-else` ladders) for:
//! - `SC3`: High-entropy obfuscated/encoded payloads via sliding-window Shannon entropy $H(X)$
//! - `SC8` / `SC9`: Concealed binary executables, bytecode, and archive smuggling via magic-byte prefix tables
//! - Passive Documentation Image Verification (`PNG`, `JPEG`, `GIF`, `WEBP`, `BMP`) to prevent false-positive binary flags
//! - `CRED1`: High-entropy live credential literals via prefix + Shannon entropy threshold
//! - `P9`: Off-screen horizontal whitespace run-length padding
//! - `TP1`: Invisible zero-width, Bidi override, and Unicode Tag block (`U+E0000..U+E007F`) smuggling via range tables
//! - `TP2`: Mixed-script homoglyph identifier poisoning via script-partition bitmasks

use super::{FindingCategory, FindingSeverity, SkillFinding};
use crate::constants::{
    SKILLSPECTOR_BENIGN_TOKEN_PREFIXES, SKILLSPECTOR_BINARY_EXTENSIONS,
    SKILLSPECTOR_BINARY_NUL_PROBE_BYTES, SKILLSPECTOR_CREDENTIAL_MIN_SUFFIX_LEN,
    SKILLSPECTOR_CREDENTIAL_PLACEHOLDERS, SKILLSPECTOR_ENTROPY_CREDENTIAL_THRESHOLD,
    SKILLSPECTOR_ENTROPY_MIN_CHAR_CLASSES, SKILLSPECTOR_ENTROPY_MIN_TOKEN_LEN,
    SKILLSPECTOR_ENTROPY_OBFUSCATED_THRESHOLD, SKILLSPECTOR_EVIDENCE_PREVIEW_LEN,
    SKILLSPECTOR_HOMOGLYPH_MIN_WORD_LEN, SKILLSPECTOR_PASSIVE_IMAGE_EXTENSIONS,
    SKILLSPECTOR_TAB_COLUMN_WIDTH, SKILLSPECTOR_WHITESPACE_PADDING_RUN_LEN,
};

struct BinaryMagicRule {
    magic: &'static [u8],
    id: &'static str,
    title: &'static str,
    severity: FindingSeverity,
}

const BINARY_MAGIC_TABLE: &[BinaryMagicRule] = &[
    BinaryMagicRule {
        magic: b"\x7fELF",
        id: "SC8",
        title: "Embedded ELF Native Binary",
        severity: FindingSeverity::Critical,
    },
    BinaryMagicRule {
        magic: &[0xFE, 0xED, 0xFA, 0xCE],
        id: "SC8",
        title: "Embedded Mach-O 32-bit Binary",
        severity: FindingSeverity::Critical,
    },
    BinaryMagicRule {
        magic: &[0xCF, 0xFA, 0xED, 0xFE],
        id: "SC8",
        title: "Embedded Mach-O 64-bit Binary",
        severity: FindingSeverity::Critical,
    },
    BinaryMagicRule {
        magic: b"MZ\x90\x00",
        id: "SC8",
        title: "Embedded PE/COFF Executable",
        severity: FindingSeverity::Critical,
    },
    BinaryMagicRule {
        magic: b"\0asm",
        id: "SC8",
        title: "Embedded WebAssembly Binary Module",
        severity: FindingSeverity::High,
    },
    BinaryMagicRule {
        magic: &[0xCA, 0xFE, 0xBA, 0xBE],
        id: "SC8",
        title: "Embedded JVM Class/Universal Binary",
        severity: FindingSeverity::High,
    },
    BinaryMagicRule {
        magic: b"PK\x03\x04",
        id: "SC9",
        title: "Concealed ZIP Archive Inside Skill Bundle",
        severity: FindingSeverity::High,
    },
    BinaryMagicRule {
        magic: &[0x1F, 0x8B, 0x08],
        id: "SC9",
        title: "Concealed GZIP Archive Inside Skill Bundle",
        severity: FindingSeverity::High,
    },
];

/// Magic byte signatures for passive documentation raster images (`passive_bmp.py`, `passive_jpeg.py`).
const PASSIVE_IMAGE_MAGIC_PREFIXES: &[&[u8]] = &[
    &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
    &[0xFF, 0xD8, 0xFF],
    b"GIF87a",
    b"GIF89a",
    b"RIFF",
    b"BM",
    &[0x00, 0x00, 0x01, 0x00],
];

const CREDENTIAL_PREFIX_TABLE: &[(&str, &str)] = &[
    ("sk-proj-", "OpenAI Project API Key"),
    ("sk-ant-", "Anthropic API Key"),
    ("ghp_", "GitHub Personal Access Token"),
    ("github_pat_", "GitHub Fine-Grained PAT"),
    ("AKIA", "AWS IAM Access Key ID"),
    ("xoxb-", "Slack Bot Token"),
    ("xoxp-", "Slack User Token"),
    ("nvapi-", "NVIDIA NIM API Key"),
    ("AIzaSy", "Google Cloud / Gemini API Key"),
];

/// Declarative table of Unicode stealth / Bidirectional override / Tag block codepoint ranges (`(start, end, is_critical_bidi_or_tag)`).
const UNICODE_STEALTH_RANGES: &[(u32, u32, bool)] = &[
    (0x061C, 0x061C, false),  // Arabic Letter Mark
    (0x200B, 0x200D, false),  // Zero-Width Space / Non-Joiner / Joiner
    (0x200E, 0x200F, false),  // Left-to-Right / Right-to-Left Mark
    (0x202A, 0x202E, true),   // LRE, RLE, PDF, LRO, RLO (Bidi Override)
    (0x2060, 0x2064, false),  // Word Joiner & Invisible Operators
    (0x2066, 0x2069, true),   // LRI, RLI, FSI, PDI (Bidi Isolate)
    (0xFEFF, 0xFEFF, false),  // Zero-Width No-Break Space (BOM)
    (0xE0000, 0xE007F, true), // Unicode Tags Block (ASCII Smuggling)
];

/// Declarative table mapping Unicode script codepoint ranges to partition bitmask flags (`Latin = 0b001, Cyrillic = 0b010, Greek = 0b100`).
const UNICODE_SCRIPT_RANGES: &[(u32, u32, u8)] = &[
    (0x0041, 0x005A, 0b001), // Basic Latin Uppercase
    (0x0061, 0x007A, 0b001), // Basic Latin Lowercase
    (0x0370, 0x03FF, 0b100), // Greek and Coptic
    (0x0400, 0x04FF, 0b010), // Cyrillic
    (0x0500, 0x052F, 0b010), // Cyrillic Supplement
];

/// Computes the exact Shannon entropy $H(X) = -\sum P(x_i) \log_2 P(x_i)$ in bits per byte (`0.0..=8.0`).
#[must_use]
pub fn shannon_entropy(bytes: &[u8]) -> f64 {
    if bytes.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let len = bytes.len() as f64;
    let mut entropy = 0.0;
    for &count in &counts {
        if count > 0 {
            let p = f64::from(count) / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

/// Verifies whether a file is a benign passive raster documentation image (`PNG`, `JPEG`, `GIF`, `WEBP`, `BMP`)
/// whose extension and magic bytes match and does not contain polyglot `<script` payloads.
#[must_use]
pub fn is_verified_passive_image(rel_path: &str, raw_bytes: &[u8]) -> bool {
    let ext = rel_path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let has_image_ext = SKILLSPECTOR_PASSIVE_IMAGE_EXTENSIONS.contains(&ext.as_str());
    let has_image_magic = PASSIVE_IMAGE_MAGIC_PREFIXES
        .iter()
        .any(|magic| raw_bytes.starts_with(magic));
    let has_polyglot_script = raw_bytes
        .windows(7)
        .any(|w| w.eq_ignore_ascii_case(b"<script"));

    has_image_ext && has_image_magic && !has_polyglot_script
}

/// Inspects raw file bytes for concealed native binaries, bytecode, or archive headers (`SC8`, `SC9`).
pub fn analyze_binary_payload(rel_path: &str, raw_bytes: &[u8]) -> Vec<SkillFinding> {
    let mut findings = Vec::new();
    if is_verified_passive_image(rel_path, raw_bytes) {
        return findings;
    }

    let lower_path = rel_path.to_ascii_lowercase();

    for rule in BINARY_MAGIC_TABLE {
        if raw_bytes.starts_with(rule.magic) {
            findings.push(SkillFinding::new(
                rule.id,
                FindingCategory::SupplyChain,
                rule.severity,
                0.98,
                rel_path,
                1,
                rule.title,
                "Skill bundle contains a compiled binary or archive header that cannot be audited as plain text.",
                format!(
                    "magic_header={:02X?}",
                    &raw_bytes[..rule.magic.len().min(8)]
                ),
                "Remove compiled binaries and archives; distribute auditable source scripts only.",
            ));
            return findings;
        }
    }

    let is_bytecode_ext = SKILLSPECTOR_BINARY_EXTENSIONS
        .iter()
        .any(|ext| lower_path.ends_with(ext));
    let has_nul_byte = raw_bytes
        .iter()
        .take(SKILLSPECTOR_BINARY_NUL_PROBE_BYTES)
        .any(|&b| b == 0);

    if is_bytecode_ext || has_nul_byte {
        findings.push(SkillFinding::new(
            "SC8",
            FindingCategory::SupplyChain,
            FindingSeverity::High,
            0.92,
            rel_path,
            1,
            "Opaque Binary or Bytecode Artifact",
            "Bundled file contains NUL bytes or a compiled bytecode extension, bypassing source-level inspection.",
            format!("file={} ({} bytes)", rel_path, raw_bytes.len()),
            "Replace opaque bytecode/binaries with human-readable source files.",
        ));
    }

    findings
}

/// Computes a 4-bit character class diversity bitmask (`lower | upper | digit | base64_punct`) branchlessly.
fn charset_diversity_mask(token: &str) -> u8 {
    token.bytes().fold(0u8, |mask, b| {
        mask | u8::from(b.is_ascii_lowercase())
            | (u8::from(b.is_ascii_uppercase()) << 1)
            | (u8::from(b.is_ascii_digit()) << 2)
            | (u8::from(matches!(b, b'+' | b'/' | b'=')) << 3)
    })
}

/// Checks if a high-entropy token is a benign URL, file path, or cryptographic digest reference.
fn is_benign_structural_token(token: &str) -> bool {
    SKILLSPECTOR_BENIGN_TOKEN_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix))
        || token.contains("://")
        || (token.contains('/')
            && !token.ends_with('=')
            && token.bytes().filter(|&b| b == b'/').count() >= 2
            && token
                .split('/')
                .any(|seg| seg.contains('.') || seg.contains('-')))
}

/// Analyzes text lines for Shannon entropy anomalies (`SC3`), high-entropy credentials (`CRED1`),
/// horizontal whitespace padding (`P9`), and Unicode stealth/homoglyph attacks (`TP1`, `TP2`).
pub fn analyze_text_entropy_and_unicode(rel_path: &str, content: &str) -> Vec<SkillFinding> {
    let mut findings = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        // 1. Horizontal whitespace padding run-length analysis (P9)
        if let Some(padding_finding) = detect_whitespace_padding_run(rel_path, line_num, line) {
            findings.push(padding_finding);
        }

        // 2. Unicode Bidi override, zero-width stealth, and mixed-script homoglyph automaton (TP1, TP2)
        analyze_line_unicode_automaton(rel_path, line_num, line, &mut findings);

        // 3. Token-level Shannon entropy analysis for obfuscated blobs (SC3) & live credentials (CRED1)
        for raw_tok in line.split_whitespace() {
            let tok = raw_tok.trim_matches(|c: char| {
                matches!(
                    c,
                    '"' | '\'' | '`' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
                )
            });

            if let Some(cred_finding) = detect_high_entropy_credential(rel_path, line_num, tok) {
                findings.push(cred_finding);
                continue;
            }

            if tok.len() >= SKILLSPECTOR_ENTROPY_MIN_TOKEN_LEN
                && tok.is_ascii()
                && !is_benign_structural_token(tok)
            {
                let entropy = shannon_entropy(tok.as_bytes());
                let class_mask = charset_diversity_mask(tok);
                if entropy >= SKILLSPECTOR_ENTROPY_OBFUSCATED_THRESHOLD
                    && class_mask.count_ones() >= SKILLSPECTOR_ENTROPY_MIN_CHAR_CLASSES
                {
                    let preview: String = tok
                        .chars()
                        .take(SKILLSPECTOR_EVIDENCE_PREVIEW_LEN)
                        .collect();
                    findings.push(SkillFinding::new(
                        "SC3",
                        FindingCategory::SupplyChain,
                        FindingSeverity::High,
                        0.89,
                        rel_path,
                        line_num,
                        "High-Entropy Obfuscated Payload",
                        "Detected a contiguous token with high Shannon entropy and multi-class character distribution characteristic of Base64 or encrypted payload smuggling.",
                        format!(
                            "H={:.2} bits/byte, len={}, sample={}...",
                            entropy,
                            tok.len(),
                            preview
                        ),
                        "Decode and replace obfuscated or Base64-encoded payloads with transparent, auditable plain text.",
                    ));
                }
            }
        }
    }

    findings
}

fn detect_whitespace_padding_run(
    rel_path: &str,
    line_num: usize,
    line: &str,
) -> Option<SkillFinding> {
    let trimmed_start = line.trim_start_matches([' ', '\t']);
    if trimmed_start.is_empty() {
        return None;
    }
    let mut run_len = 0usize;
    let mut max_mid_run = 0usize;
    let mut seen_non_ws = false;

    for ch in line.chars() {
        if matches!(ch, ' ' | '\t') {
            if seen_non_ws {
                run_len += if ch == '\t' {
                    SKILLSPECTOR_TAB_COLUMN_WIDTH
                } else {
                    1
                };
            }
        } else {
            max_mid_run = max_mid_run.max(run_len);
            run_len = 0;
            seen_non_ws = true;
        }
    }

    let leading_cols: usize = line
        .chars()
        .take_while(|&c| matches!(c, ' ' | '\t'))
        .map(|c| {
            if c == '\t' {
                SKILLSPECTOR_TAB_COLUMN_WIDTH
            } else {
                1
            }
        })
        .sum();

    let effective_run = max_mid_run.max(leading_cols);
    (effective_run >= SKILLSPECTOR_WHITESPACE_PADDING_RUN_LEN).then(|| {
        let sample: String = trimmed_start
            .chars()
            .take(SKILLSPECTOR_EVIDENCE_PREVIEW_LEN)
            .collect();
        SkillFinding::new(
            "P9",
            FindingCategory::PromptInjection,
            FindingSeverity::Medium,
            0.84,
            rel_path,
            line_num,
            "Off-Screen Horizontal Whitespace Padding",
            "Line contains an abnormally long contiguous whitespace run followed by trailing text, often used to hide instructions beyond viewport width.",
            format!("contiguous_whitespace_cols={}, trailing='{}'", effective_run, sample),
            "Remove excessive horizontal whitespace padding so all instructions remain visible in standard viewports.",
        )
    })
}

fn detect_high_entropy_credential(
    rel_path: &str,
    line_num: usize,
    token: &str,
) -> Option<SkillFinding> {
    for &(prefix, label) in CREDENTIAL_PREFIX_TABLE {
        if let Some(rest) = token.strip_prefix(prefix) {
            if rest.len() >= SKILLSPECTOR_CREDENTIAL_MIN_SUFFIX_LEN
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                let lower = rest.to_ascii_lowercase();
                if SKILLSPECTOR_CREDENTIAL_PLACEHOLDERS
                    .iter()
                    .any(|p| lower.contains(p))
                {
                    return None;
                }
                let entropy = shannon_entropy(rest.as_bytes());
                if entropy >= SKILLSPECTOR_ENTROPY_CREDENTIAL_THRESHOLD {
                    let masked = format!("{}***[REDACTED]", prefix);
                    return Some(SkillFinding::new(
                        "CRED1",
                        FindingCategory::CredentialExposure,
                        FindingSeverity::Critical,
                        0.95,
                        rel_path,
                        line_num,
                        "Hardcoded High-Entropy Credential",
                        &format!(
                            "Detected a live-formatted {} with high Shannon entropy ({:.2} bits/byte).",
                            label, entropy
                        ),
                        masked,
                        "Remove hardcoded credentials immediately and reference environment variables instead.",
                    ));
                }
            }
        }
    }
    None
}

fn analyze_line_unicode_automaton(
    rel_path: &str,
    line_num: usize,
    line: &str,
    findings: &mut Vec<SkillFinding>,
) {
    let mut stealth_codepoints: Vec<u32> = Vec::new();
    let mut has_critical_bidi_or_tag = false;

    for ch in line.chars() {
        let cp = ch as u32;
        if let Some(&(_, _, is_critical)) = UNICODE_STEALTH_RANGES
            .iter()
            .find(|&&(lo, hi, _)| (lo..=hi).contains(&cp))
        {
            stealth_codepoints.push(cp);
            has_critical_bidi_or_tag |= is_critical;
        }
    }

    if !stealth_codepoints.is_empty() {
        let hex_list: Vec<String> = stealth_codepoints
            .iter()
            .take(6)
            .map(|cp| format!("U+{:04X}", cp))
            .collect();
        let severity = if has_critical_bidi_or_tag
            || stealth_codepoints.len() >= SKILLSPECTOR_ENTROPY_MIN_CHAR_CLASSES as usize
        {
            FindingSeverity::Critical
        } else {
            FindingSeverity::High
        };
        findings.push(SkillFinding::new(
            "TP1",
            FindingCategory::ToolPoisoning,
            severity,
            0.96,
            rel_path,
            line_num,
            "Unicode Stealth / Bidi Override Injection",
            "Detected invisible zero-width, Bidirectional text override, or Unicode Tag block characters used to smuggle hidden prompt directives.",
            format!(
                "count={}, codepoints=[{}]",
                stealth_codepoints.len(),
                hex_list.join(", ")
            ),
            "Strip all invisible zero-width, Bidi control, and Unicode Tag block codepoints from the file.",
        ));
    }

    // Script-partition bitmask check per identifier word for homoglyph detection (TP2)
    for word in line.split(|c: char| !c.is_alphabetic() && c != '_' && c != '-') {
        if word.len() < SKILLSPECTOR_HOMOGLYPH_MIN_WORD_LEN {
            continue;
        }
        let script_mask = word.chars().fold(0u8, |mask, ch| {
            let cp = ch as u32;
            UNICODE_SCRIPT_RANGES
                .iter()
                .find(|&&(lo, hi, _)| (lo..=hi).contains(&cp))
                .map_or(mask, |&(_, _, bit)| mask | bit)
        });
        if script_mask.count_ones() >= 2 {
            findings.push(SkillFinding::new(
                "TP2",
                FindingCategory::ToolPoisoning,
                FindingSeverity::High,
                0.94,
                rel_path,
                line_num,
                "Mixed-Script Homoglyph Identifier",
                "Identifier mixes Latin, Cyrillic, or Greek scripts within a single token to spoof legitimate tool or variable names.",
                format!("token='{}', script_mask=0b{:03b}", word, script_mask),
                "Normalize all identifiers to a single consistent script (ASCII/Latin).",
            ));
        }
    }
}
