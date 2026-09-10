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
}
