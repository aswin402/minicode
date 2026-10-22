//! Centralized UTF-8 safe string manipulation and redaction utilities.

/// Slices a string up to `max_chars` unicode characters without splitting multi-byte codepoints.
///
/// Returns a subslice `&str` pointing to the exact character boundary.
/// If `s` contains fewer than or equal to `max_chars` characters, returns the full string slice.
#[must_use]
pub fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

use unicode_width::UnicodeWidthStr;

/// Truncates a string to at most `max_chars` unicode characters, appending an ellipsis (`…`)
/// if truncation occurred.
#[must_use]
pub fn truncate_display(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }

    let keep_chars = max_chars.saturating_sub(1);
    let mut truncated = String::with_capacity(s.len().min(max_chars * 4) + 3);
    for ch in s.chars().take(keep_chars) {
        truncated.push(ch);
    }
    truncated.push('…');
    truncated
}

/// Truncates a string to fit within `max_cols` visual display columns.
/// Handles CJK full-width characters and emojis safely without breaking characters.
#[must_use]
pub fn truncate_cols(s: &str, max_cols: usize, suffix: &str) -> String {
    if UnicodeWidthStr::width(s) <= max_cols {
        return s.to_string();
    }
    let suffix_width = UnicodeWidthStr::width(suffix);
    let target = max_cols.saturating_sub(suffix_width);
    let mut width = 0;
    let mut end_idx = 0;
    for (idx, ch) in s.char_indices() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w > target {
            break;
        }
        width += w;
        end_idx = idx + ch.len_utf8();
    }
    format!("{}{}", &s[..end_idx], suffix)
}

/// Truncates a string to at most `max_chars` characters, appending `"..."` if truncated.
#[must_use]
pub fn truncate_ellipsis(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    let keep_chars = max_chars.saturating_sub(3);
    if keep_chars == 0 {
        return "...".chars().take(max_chars).collect();
    }
    let mut truncated = String::with_capacity(s.len().min(max_chars * 4) + 3);
    for ch in s.chars().take(keep_chars) {
        truncated.push(ch);
    }
    truncated.push_str("...");
    truncated
}

/// Masks sensitive credentials or tokens, leaving only the trailing `visible_tail` characters.
///
/// E.g. `mask_secret("sk-1234567890abcdef", 4)` returns `****************cdef`.
#[must_use]
pub fn mask_secret(secret: &str, visible_tail: usize) -> String {
    if secret.is_empty() {
        return String::new();
    }

    let char_count = secret.chars().count();
    if char_count <= visible_tail {
        return "*".repeat(char_count.max(3));
    }

    let masked_count = char_count - visible_tail;
    let mut result = String::with_capacity(char_count);
    for _ in 0..masked_count {
        result.push('*');
    }
    for ch in secret.chars().skip(masked_count) {
        result.push(ch);
    }
    result
}

/// Checks if a string contains `word` as a distinct token, bounded by whitespace or punctuation.
#[must_use]
pub fn has_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w.eq_ignore_ascii_case(word))
}

/// Checks if a string contains any of `words` as a distinct token.
#[must_use]
pub fn has_any_word(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| has_word(text, w))
}

/// Strips ANSI terminal control and color escape sequences (CSI `\x1b[...`, OSC `\x1b]...`,
/// and G0/G1 charset `\x1b(X`) using a single-pass $O(N)$ state machine with zero regex compilation.
#[must_use]
pub fn strip_ansi_codes(input: &str) -> String {
    if !input.contains('\x1b') {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            out.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('[') => {
                // CSI sequence: ESC [ <params 0x30..=0x3F> <intermediates 0x20..=0x2F> <final 0x40..=0x7E>
                let _ = chars.next();
                for c in chars.by_ref() {
                    let b = c as u32;
                    if (0x40..=0x7E).contains(&b) {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC sequence: ESC ] ... terminated by BEL (\x07) or ST (ESC \)
                let _ = chars.next();
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        let _ = chars.next();
                        break;
                    }
                }
            }
            Some('(' | ')' | '*' | '+') => {
                // Charset designation: ESC ( B, ESC ) 0, etc.
                let _ = chars.next();
                let _ = chars.next();
            }
            Some(_) => {
                // Two-byte escape sequence (e.g. ESC M, ESC =, ESC >)
                let _ = chars.next();
            }
            None => {}
        }
    }

    out
}

/// Strips inline and trailing code comments (`//`, `/* ... */`, and Python `#` comments)
/// using a lexical scanner that respects string literals (e.g. `"https://..."` is preserved).
#[must_use]
pub fn strip_line_comments(line: &str) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return String::new();
    }
    if trimmed.starts_with('#') && !trimmed.starts_with("#[") && !trimmed.starts_with("#![") {
        return String::new();
    }

    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        if let Some(q) = in_quote {
            out.push(ch);
            if ch == '\\' {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            } else if ch == q {
                in_quote = None;
            }
            continue;
        }

        if ch == '/' {
            if chars.peek() == Some(&'/') {
                break;
            }
            if chars.peek() == Some(&'*') {
                let _ = chars.next();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'/') {
                        let _ = chars.next();
                        break;
                    }
                }
                continue;
            }
        }

        if ch == '"' || ch == '`' {
            in_quote = Some(ch);
            out.push(ch);
            continue;
        }

        if ch == '\'' {
            // Check if there is a closing single quote on this line (distinguishes strings/chars from Rust lifetimes `'a`)
            if has_closing_quote(chars.clone(), '\'') {
                in_quote = Some('\'');
            }
            out.push(ch);
            continue;
        }

        out.push(ch);
    }

    out
}

/// Strips both string literal contents (`"..."`, `'...'`, `` `...` ``) and code comments
/// (`//`, `/* ... */`, Python `#`) using a single-pass lexical state machine so code pattern
/// checks never false-positive on comments or string literals.
#[must_use]
pub fn strip_code_strings_and_comments(line: &str) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return String::new();
    }
    if trimmed.starts_with('#') && !trimmed.starts_with("#[") && !trimmed.starts_with("#![") {
        return String::new();
    }

    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '/' {
            if chars.peek() == Some(&'/') {
                break;
            }
            if chars.peek() == Some(&'*') {
                let _ = chars.next();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'/') {
                        let _ = chars.next();
                        break;
                    }
                }
                continue;
            }
        }

        if ch == '"' || ch == '`' {
            out.push(ch);
            while let Some(c) = chars.next() {
                if c == '\\' {
                    let _ = chars.next();
                } else if c == ch {
                    out.push(ch);
                    break;
                }
            }
            continue;
        }

        if ch == '\'' && has_closing_quote(chars.clone(), '\'') {
            out.push('\'');
            while let Some(c) = chars.next() {
                if c == '\\' {
                    let _ = chars.next();
                } else if c == '\'' {
                    out.push('\'');
                    break;
                }
            }
            continue;
        }

        out.push(ch);
    }

    out
}

fn has_closing_quote(mut chars: impl Iterator<Item = char>, quote: char) -> bool {
    while let Some(c) = chars.next() {
        if c == '\\' {
            let _ = chars.next();
        } else if c == quote {
            return true;
        }
    }
    false
}

/// Matches `text` against a glob/wildcard `pattern` (`*` matches 0+ characters, `?` matches 1 character)
/// using a two-pointer backtracking state machine with zero regex compilation.
#[must_use]
pub fn wildcard_match(text: &str, pattern: &str) -> bool {
    wildcard_match_internal(text.as_bytes(), pattern.as_bytes(), false)
}

/// Case-insensitive wildcard matcher (`*` and `?`) with zero regex compilation.
#[must_use]
pub fn wildcard_match_case_insensitive(text: &str, pattern: &str) -> bool {
    wildcard_match_internal(text.as_bytes(), pattern.as_bytes(), true)
}

fn wildcard_match_internal(text: &[u8], pattern: &[u8], ignore_case: bool) -> bool {
    let mut t_idx = 0;
    let mut p_idx = 0;
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0;

    while t_idx < text.len() {
        if p_idx < pattern.len()
            && (pattern[p_idx] == b'?'
                || (ignore_case && pattern[p_idx].eq_ignore_ascii_case(&text[t_idx]))
                || (!ignore_case && pattern[p_idx] == text[t_idx]))
        {
            t_idx += 1;
            p_idx += 1;
        } else if p_idx < pattern.len() && pattern[p_idx] == b'*' {
            star_idx = Some(p_idx);
            match_idx = t_idx;
            p_idx += 1;
        } else if let Some(s_idx) = star_idx {
            p_idx = s_idx + 1;
            match_idx += 1;
            t_idx = match_idx;
        } else {
            return false;
        }
    }

    while p_idx < pattern.len() && pattern[p_idx] == b'*' {
        p_idx += 1;
    }

    p_idx == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_chars_ascii() {
        assert_eq!(truncate_chars("hello world", 5), "hello");
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert_eq!(truncate_chars("", 5), "");
        assert_eq!(truncate_chars("abc", 0), "");
    }

    #[test]
    fn test_truncate_chars_multibyte_utf8() {
        // "🦀 minicode 🚀" -> '🦀' (4 bytes), ' ' (1), 'm' (1)...
        let text = "🦀 minicode 🚀";
        assert_eq!(truncate_chars(text, 1), "🦀");
        assert_eq!(truncate_chars(text, 2), "🦀 ");
        assert_eq!(truncate_chars(text, 10), "🦀 minicode");
        assert_eq!(truncate_chars(text, 20), "🦀 minicode 🚀");
    }

    #[test]
    fn test_truncate_display() {
        assert_eq!(truncate_display("hello", 5), "hello");
        assert_eq!(truncate_display("hello world", 6), "hello…");
        assert_eq!(truncate_display("🦀🚀🌟🔥", 3), "🦀🚀…");
        assert_eq!(truncate_display("", 5), "");
        assert_eq!(truncate_display("a", 1), "a");
    }

    #[test]
    fn test_truncate_cols() {
        let s = "你好世界";
        assert_eq!(truncate_cols(s, 5, "..."), "你...");
        assert_eq!(truncate_cols(s, 6, "…"), "你好…");
        assert_eq!(truncate_cols(s, 8, "…"), "你好世界");
    }

    #[test]
    fn test_truncate_ellipsis() {
        assert_eq!(truncate_ellipsis("hello", 5), "hello");
        assert_eq!(truncate_ellipsis("hello world", 8), "hello...");
        assert_eq!(truncate_ellipsis("short", 10), "short");
        assert_eq!(truncate_ellipsis("abcde", 4), "a...");
        assert_eq!(truncate_ellipsis("abcde", 2), "..");
    }

    #[test]
    fn test_mask_secret() {
        assert_eq!(mask_secret("sk-1234567890abcdef", 4), "***************cdef");
        assert_eq!(mask_secret("short", 10), "*****");
        assert_eq!(mask_secret("", 4), "");
        assert_eq!(mask_secret("abcdef", 2), "****ef");
    }

    #[test]
    fn test_has_word() {
        assert!(has_word("style the sidebar with css", "css"));
        assert!(!has_word("process background tasks", "css"));
        assert!(has_word("modern UI with buttons", "ui"));
        assert!(!has_word("build cargo project", "ui"));
        assert!(has_word("start the dev-server", "dev"));
        assert!(has_word(
            "where is process_payment defined?",
            "process_payment"
        ));
    }
}
