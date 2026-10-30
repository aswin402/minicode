use std::path::Path;
use tree_sitter::{Language, Node, Parser};

/// Detailed description of a syntax error detected by Tree-sitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxErrorDetail {
    pub line: usize,
    pub column: usize,
    pub is_missing: bool,
    pub kind: String,
    pub snippet: String,
}

/// In-memory AST pre-write barrier that prevents syntax-breaking edits from corrupting disk files.
pub struct SyntaxGuard;

impl SyntaxGuard {
    /// Maps a file extension to its corresponding Tree-sitter Language definition.
    pub fn language_for_extension(ext: &str) -> Option<Language> {
        match ext {
            "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
            "py" => Some(tree_sitter_python::LANGUAGE.into()),
            "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
            "ts" | "mts" | "cts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
            _ => None,
        }
    }

    /// Maps a file path to its corresponding Tree-sitter Language definition.
    pub fn language_for_path(path: &Path) -> Option<Language> {
        let ext = path.extension().and_then(|e| e.to_str())?;
        Self::language_for_extension(ext)
    }

    /// Recursively walks the AST to locate the most specific ERROR or MISSING node.
    fn find_first_error(node: Node, code: &str) -> Option<SyntaxErrorDetail> {
        if node.is_error() || node.is_missing() {
            if node.is_error() {
                let mut inner_cursor = node.walk();
                for child in node.children(&mut inner_cursor) {
                    if child.is_error() || child.is_missing() || child.has_error() {
                        if let Some(inner_err) = Self::find_first_error(child, code) {
                            return Some(inner_err);
                        }
                    }
                }
                if let Some(jsx_err) = Self::find_unmatched_jsx_tag_in_error(node, code) {
                    return Some(jsx_err);
                }
                if Self::is_benign_jsx_ampersand_error(node, code) {
                    return None;
                }
            }

            let start = node.start_position();
            let snippet = code
                .lines()
                .nth(start.row)
                .unwrap_or("")
                .trim_end()
                .to_string();

            let kind = if node.is_missing() {
                format!("missing '{}'", node.kind())
            } else {
                let text = node.utf8_text(code.as_bytes()).unwrap_or("<syntax>");
                let display_text = if text.is_empty() {
                    "<token>".to_string()
                } else {
                    crate::utils::strings::truncate_display(text, 30)
                };
                format!("unexpected token '{}'", display_text)
            };

            return Some(SyntaxErrorDetail {
                line: start.row + 1,
                column: start.column + 1,
                is_missing: node.is_missing(),
                kind,
                snippet,
            });
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.has_error() {
                if let Some(err) = Self::find_first_error(child, code) {
                    return Some(err);
                }
            }
        }
        None
    }

    /// Returns true when a Tree-sitter `ERROR` node is solely a bare `&` (or `&word` without `;`)
    /// inside JSX element/fragment text content. `tree-sitter-typescript`'s TSX grammar requires
    /// `&` in `jsx_text` to match an HTML character entity (`&[a-zA-Z0-9#]+;`), whereas React,
    /// TypeScript (`tsc`), and Vite/esbuild permit bare `&` in JSX text (e.g. `<Label>Console & network</Label>`).
    fn is_benign_jsx_ampersand_error(node: Node, code: &str) -> bool {
        if !node.is_error() || node.is_missing() {
            return false;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        let in_jsx_container = matches!(parent.kind(), "jsx_element" | "jsx_fragment")
            || parent
                .parent()
                .is_some_and(|gp| matches!(gp.kind(), "jsx_element" | "jsx_fragment"));
        if !in_jsx_container {
            return false;
        }
        let Ok(text) = node.utf8_text(code.as_bytes()) else {
            return false;
        };
        let trimmed = text.trim();
        trimmed.starts_with('&') && !trimmed.contains(['<', '>', '{', '}'])
    }

    /// Inspects an `ERROR` AST subtree for unreduced `jsx_opening_element` / `jsx_closing_element`
    /// nodes and returns a `SyntaxErrorDetail` pointing to the unmatched tag if found.
    fn find_unmatched_jsx_tag_in_error(error_node: Node, code: &str) -> Option<SyntaxErrorDetail> {
        let mut unreduced_tags: Vec<(bool, String, Node)> = Vec::new();
        Self::collect_unreduced_jsx_tags(error_node, code, &mut unreduced_tags);
        if unreduced_tags.is_empty() {
            return None;
        }

        let mut open_stack: Vec<(String, Node)> = Vec::new();
        let mut unmatched_closes: Vec<(String, Node)> = Vec::new();

        for (is_open, tag_name, tag_node) in unreduced_tags {
            if is_open {
                open_stack.push((tag_name, tag_node));
            } else if let Some(pos) = open_stack
                .iter()
                .rposition(|(open_name, _)| *open_name == tag_name)
            {
                open_stack.remove(pos);
            } else {
                unmatched_closes.push((tag_name, tag_node));
            }
        }

        if let Some((tag_name, open_node)) = open_stack.pop() {
            let start = open_node.start_position();
            let snippet = code
                .lines()
                .nth(start.row)
                .unwrap_or("")
                .trim_end()
                .to_string();
            return Some(SyntaxErrorDetail {
                line: start.row + 1,
                column: start.column + 1,
                is_missing: true,
                kind: format!(
                    "unclosed JSX opening tag '<{tag_name}>' (missing matching '</{tag_name}>')"
                ),
                snippet,
            });
        }

        if let Some((tag_name, close_node)) = unmatched_closes.into_iter().next() {
            let start = close_node.start_position();
            let snippet = code
                .lines()
                .nth(start.row)
                .unwrap_or("")
                .trim_end()
                .to_string();
            return Some(SyntaxErrorDetail {
                line: start.row + 1,
                column: start.column + 1,
                is_missing: false,
                kind: format!(
                    "unexpected JSX closing tag '</{tag_name}>' with no matching '<{tag_name}>'"
                ),
                snippet,
            });
        }

        None
    }

    fn collect_unreduced_jsx_tags<'a>(
        node: Node<'a>,
        code: &str,
        out: &mut Vec<(bool, String, Node<'a>)>,
    ) {
        let kind = node.kind();
        // Skip complete, already-balanced jsx_element subtrees
        if kind == "jsx_element" || kind == "jsx_self_closing_element" {
            return;
        }
        if kind == "jsx_opening_element" || kind == "jsx_closing_element" {
            if let Ok(text) = node.utf8_text(code.as_bytes()) {
                let trimmed = text.trim();
                if !trimmed.ends_with("/>") {
                    let inner = trimmed
                        .trim_start_matches('<')
                        .trim_start_matches('/')
                        .trim();
                    let tag_name: String = inner
                        .chars()
                        .take_while(|c| {
                            c.is_ascii_alphanumeric()
                                || *c == '.'
                                || *c == '_'
                                || *c == '-'
                                || *c == ':'
                        })
                        .collect();
                    if !tag_name.is_empty() {
                        out.push((kind == "jsx_opening_element", tag_name, node));
                    }
                }
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::collect_unreduced_jsx_tags(child, code, out);
        }
    }

    /// Computes the 1-based modified line span `(first_line, last_line, removed_text, added_text)`
    /// in `updated` by stripping the common line prefix and suffix between `original` and `updated`.
    fn find_modified_line_span(
        original: &str,
        updated: &str,
    ) -> Option<(usize, usize, String, String)> {
        let orig_lines: Vec<&str> = original.lines().collect();
        let new_lines: Vec<&str> = updated.lines().collect();
        if orig_lines.is_empty() || new_lines.is_empty() || original == updated {
            return None;
        }

        let mut prefix = 0;
        while prefix < orig_lines.len()
            && prefix < new_lines.len()
            && orig_lines[prefix] == new_lines[prefix]
        {
            prefix += 1;
        }

        let mut orig_suffix = orig_lines.len();
        let mut new_suffix = new_lines.len();
        while orig_suffix > prefix
            && new_suffix > prefix
            && orig_lines[orig_suffix - 1] == new_lines[new_suffix - 1]
        {
            orig_suffix -= 1;
            new_suffix -= 1;
        }

        let first_line = (prefix + 1).min(new_lines.len());
        let last_line = new_suffix.max(first_line).min(new_lines.len());
        let removed = orig_lines[prefix..orig_suffix].join("\n");
        let added = new_lines[prefix..new_suffix].join("\n");
        Some((first_line, last_line, removed, added))
    }

    /// Counts JSX/HTML opening vs closing tags in a code slice, ignoring self-closing and void tags
    /// as well as TypeScript generics (`Foo<Bar>` where `<` is immediately preceded by an identifier char).
    fn count_jsx_tag_balance(snippet: &str) -> std::collections::BTreeMap<String, isize> {
        const VOID_TAGS: &[&str] = &[
            "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
            "source", "track", "wbr",
        ];
        let mut counts = std::collections::BTreeMap::new();
        let bytes = snippet.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i] == b'<' && i + 1 < len {
                let prev_is_ident = i > 0
                    && (bytes[i - 1].is_ascii_alphanumeric()
                        || bytes[i - 1] == b'_'
                        || bytes[i - 1] == b'$'
                        || bytes[i - 1] == b'>');
                // Note: `prev == b'>'` IS allowed for adjacent JSX tags like `<div><span>`, so exclude `b'>'`
                let prev_is_generic_ident = i > 0
                    && (bytes[i - 1].is_ascii_alphanumeric()
                        || bytes[i - 1] == b'_'
                        || bytes[i - 1] == b'$');
                let _ = prev_is_ident;
                if prev_is_generic_ident {
                    i += 1;
                    continue;
                }

                let is_closing = bytes[i + 1] == b'/';
                let name_start = if is_closing { i + 2 } else { i + 1 };
                if name_start >= len || !bytes[name_start].is_ascii_alphabetic() {
                    i += 1;
                    continue;
                }

                let mut name_end = name_start;
                while name_end < len
                    && (bytes[name_end].is_ascii_alphanumeric()
                        || bytes[name_end] == b'.'
                        || bytes[name_end] == b'_'
                        || bytes[name_end] == b'-'
                        || bytes[name_end] == b':')
                {
                    name_end += 1;
                }
                let tag_name = String::from_utf8_lossy(&bytes[name_start..name_end]).to_string();

                // Scan to the end of the tag `>` at brace_depth == 0 and outside quotes
                let mut j = name_end;
                let mut brace_depth: usize = 0;
                let mut in_quote: Option<u8> = None;
                let mut found_end: Option<(usize, bool)> = None;

                while j < len {
                    let b = bytes[j];
                    if let Some(q) = in_quote {
                        if b == b'\\' {
                            j += 2;
                            continue;
                        }
                        if b == q {
                            in_quote = None;
                        }
                    } else {
                        match b {
                            b'"' | b'\'' | b'`' => in_quote = Some(b),
                            b'{' => brace_depth += 1,
                            b'}' => brace_depth = brace_depth.saturating_sub(1),
                            b'>' if brace_depth == 0 => {
                                let mut k = j;
                                while k > name_end && bytes[k - 1].is_ascii_whitespace() {
                                    k -= 1;
                                }
                                let self_closing = k > name_end && bytes[k - 1] == b'/';
                                found_end = Some((j, self_closing));
                                break;
                            }
                            _ => {}
                        }
                    }
                    j += 1;
                }

                if let Some((end_idx, self_closing)) = found_end {
                    if !self_closing && !VOID_TAGS.contains(&tag_name.as_str()) {
                        let delta = if is_closing { -1 } else { 1 };
                        *counts.entry(tag_name).or_insert(0) += delta;
                    }
                    i = end_idx + 1;
                    continue;
                }
            }
            i += 1;
        }

        counts
    }

    /// Detects whether an edit between `removed` and `added` introduced an unbalanced JSX tag or delimiter.
    fn detect_edit_imbalance(removed: &str, added: &str, is_jsx_or_tsx: bool) -> Option<String> {
        if is_jsx_or_tsx {
            let rem_tags = Self::count_jsx_tag_balance(removed);
            let add_tags = Self::count_jsx_tag_balance(added);
            let all_keys: std::collections::BTreeSet<&String> =
                rem_tags.keys().chain(add_tags.keys()).collect();
            for tag in all_keys {
                let r_bal = rem_tags.get(tag).copied().unwrap_or(0);
                let a_bal = add_tags.get(tag).copied().unwrap_or(0);
                if a_bal > r_bal {
                    return Some(format!(
                        "unclosed '<{tag}>' introduced here without a matching '</{tag}>' in the same edit"
                    ));
                } else if a_bal < r_bal {
                    return Some(format!(
                        "unmatched '</{tag}>' (or removed '<{tag}>') without its matching counterpart in the same edit"
                    ));
                }
            }
        }

        let count_delim = |s: &str, open: char, close: char| -> isize {
            let mut bal = 0isize;
            for ch in s.chars() {
                if ch == open {
                    bal += 1;
                } else if ch == close {
                    bal -= 1;
                }
            }
            bal
        };

        for (open, close, label) in [
            ('{', '}', "brace"),
            ('(', ')', "parenthesis"),
            ('[', ']', "bracket"),
        ] {
            let r_bal = count_delim(removed, open, close);
            let a_bal = count_delim(added, open, close);
            if a_bal > r_bal {
                return Some(format!(
                    "unclosed '{open}' ({label}) introduced here without a matching '{close}' in the same edit"
                ));
            } else if a_bal < r_bal {
                return Some(format!(
                    "unmatched '{close}' ({label}) introduced here without a matching '{open}' in the same edit"
                ));
            }
        }

        None
    }

    /// Parses a file's content and returns syntax error details if any syntax error exists.
    #[must_use]
    pub fn check_syntax(path: &Path, content: &str) -> Option<SyntaxErrorDetail> {
        let lang = Self::language_for_path(path)?;
        let mut parser = Parser::new();
        parser.set_language(&lang).ok()?;
        let tree = parser.parse(content, None)?;
        if tree.root_node().has_error() {
            Self::find_first_error(tree.root_node(), content)
        } else {
            None
        }
    }

    /// Extracts brand icon names imported from `lucide-react` (e.g. Github, Twitter, Discord)
    /// by walking `import_statement` nodes in the Tree-sitter TSX/JSX AST (ignoring comments and strings).
    pub fn extract_lucide_brand_imports(content: &str) -> Vec<String> {
        if !content.contains("lucide-react") {
            return Vec::new();
        }

        const KNOWN_BRAND_ICONS: &[&str] = &[
            "github",
            "gitlab",
            "twitter",
            "discord",
            "linkedin",
            "facebook",
            "instagram",
            "youtube",
            "twitch",
            "slack",
            "reddit",
            "telegram",
            "tiktok",
            "whatsapp",
        ];

        let mut parser = Parser::new();
        let lang: Language = tree_sitter_typescript::LANGUAGE_TSX.into();
        if parser.set_language(&lang).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(content, None) else {
            return Vec::new();
        };

        let mut brands = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if child.kind() != "import_statement" {
                continue;
            }
            let Some(source_node) = child.child_by_field_name("source") else {
                continue;
            };
            let Ok(source_raw) = source_node.utf8_text(content.as_bytes()) else {
                continue;
            };
            let source_clean = source_raw.trim().trim_matches(|c| c == '"' || c == '\'');
            if source_clean != "lucide-react" {
                continue;
            }

            // Walk the import_statement subtree to collect imported symbol names
            Self::collect_imported_brand_specifiers(
                child,
                content.as_bytes(),
                KNOWN_BRAND_ICONS,
                &mut brands,
            );
        }

        brands
    }

    fn collect_imported_brand_specifiers(
        node: Node,
        bytes: &[u8],
        known_brands: &[&str],
        brands: &mut Vec<String>,
    ) {
        if node.kind() == "import_specifier" {
            let ident_node = node.child_by_field_name("name").or_else(|| node.child(0));
            if let Some(id_node) = ident_node {
                if let Ok(ident) = id_node.utf8_text(bytes) {
                    let ident = ident.trim();
                    let lower = ident.to_ascii_lowercase();
                    let is_brand = known_brands
                        .iter()
                        .any(|&brand| lower == brand || lower.starts_with(brand));
                    if is_brand
                        && !brands
                            .iter()
                            .any(|b: &String| b.eq_ignore_ascii_case(ident))
                    {
                        brands.push(ident.to_string());
                    }
                }
            }
            return;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::collect_imported_brand_specifiers(child, bytes, known_brands, brands);
        }
    }

    /// Strips `/* ... */` comments from CSS while preserving quoted string literals.
    fn strip_css_comments(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
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

            if ch == '"' || ch == '\'' {
                in_quote = Some(ch);
                out.push(ch);
                continue;
            }

            if ch == '/' && chars.peek() == Some(&'*') {
                let _ = chars.next();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'/') {
                        let _ = chars.next();
                        break;
                    }
                }
                continue;
            }

            out.push(ch);
        }
        out
    }

    /// Pure lexical CSS brace-stack state machine that detects unlayered universal box-model
    /// resets (`* { margin: 0; padding: 0; }`) outside of `@layer`.
    /// In Tailwind CSS v4, utilities live in `@layer utilities`; any unlayered `*` reset
    /// overrides every `@layer utilities` padding/margin class across the entire application.
    fn find_unlayered_universal_box_reset(clean_css: &str) -> Option<String> {
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum CssBlock {
            Layer,
            AtRule,
            StyleRule { selector: String, in_layer: bool },
        }

        const BOX_SPACING_PROPS: &[&str] = &[
            "margin",
            "margin-top",
            "margin-right",
            "margin-bottom",
            "margin-left",
            "margin-inline",
            "margin-block",
            "padding",
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
            "padding-inline",
            "padding-block",
        ];

        const ZERO_VALUES: &[&str] = &["0", "0px", "0rem", "0em", "0%"];

        let mut stack: Vec<CssBlock> = Vec::new();
        let mut buf = String::new();
        let mut chars = clean_css.chars().peekable();
        let mut in_quote: Option<char> = None;

        while let Some(ch) = chars.next() {
            if let Some(q) = in_quote {
                buf.push(ch);
                if ch == '\\' {
                    if let Some(escaped) = chars.next() {
                        buf.push(escaped);
                    }
                } else if ch == q {
                    in_quote = None;
                }
                continue;
            }

            match ch {
                '"' | '\'' => {
                    in_quote = Some(ch);
                    buf.push(ch);
                }
                '{' => {
                    let header = buf.trim().to_string();
                    buf.clear();
                    if let Some(rest) = header.strip_prefix('@') {
                        let kw = rest
                            .split(|c: char| c.is_whitespace() || c == '(')
                            .next()
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        if kw == "layer" {
                            stack.push(CssBlock::Layer);
                        } else {
                            stack.push(CssBlock::AtRule);
                        }
                    } else {
                        let in_layer = stack.contains(&CssBlock::Layer);
                        stack.push(CssBlock::StyleRule {
                            selector: header,
                            in_layer,
                        });
                    }
                }
                '}' => {
                    let body = std::mem::take(&mut buf);
                    if let Some(CssBlock::StyleRule { selector, in_layer }) = stack.pop() {
                        if !in_layer {
                            let targets_universal = selector.split(',').any(|part| {
                                matches!(
                                    part.trim(),
                                    "*" | "*::before" | "*::after" | "*:before" | "*:after"
                                )
                            });
                            if targets_universal {
                                for decl in body.split(';') {
                                    if let Some((prop_raw, val_raw)) = decl.split_once(':') {
                                        let prop = prop_raw.trim().to_ascii_lowercase();
                                        let val = val_raw.trim().to_ascii_lowercase();
                                        let resets_to_zero = val
                                            .split_whitespace()
                                            .any(|tok| ZERO_VALUES.contains(&tok));
                                        if BOX_SPACING_PROPS.contains(&prop.as_str())
                                            && resets_to_zero
                                        {
                                            let compact_sel = selector
                                                .split_whitespace()
                                                .collect::<Vec<_>>()
                                                .join(" ");
                                            return Some(format!(
                                                "{compact_sel} {{ {prop}: {}; }}",
                                                val_raw.trim()
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                ';' => {
                    let inside_style_rule = stack
                        .last()
                        .is_some_and(|b| matches!(b, CssBlock::StyleRule { .. }));
                    if inside_style_rule {
                        buf.push(';');
                    } else {
                        buf.clear();
                    }
                }
                _ => {
                    buf.push(ch);
                }
            }
        }

        None
    }

    /// Checks CSS files for fatal semantic errors using lexical & cascade-layer analysis:
    /// 1. Double-wrapped color functions (`oklch(var(--foo))` / `hsl(var(--foo))` / `rgb(var(--foo))`)
    ///    when `--foo` is defined in the same stylesheet with a complete color value (`oklch(...)`, `hsl(...)`, `rgb(...)`, `#...`).
    ///    This evaluates to `oklch(oklch(...))` at runtime, causing all colors/backgrounds to collapse to transparent!
    /// 2. `@apply border-border` in Tailwind v4 stylesheets when neither `--color-border` nor `--border` is defined.
    /// 3. Unlayered universal box-model reset (`* { margin: 0; padding: 0; }`) in Tailwind v4 (`@import "tailwindcss"`),
    ///    which overrides all `@layer utilities` spacing classes (`px-*`, `py-*`, `pt-*`, `mx-auto`, `p-*`).
    pub fn check_css_semantic_barrier(path: &Path, content: &str) -> Result<(), String> {
        let clean_css = Self::strip_css_comments(content);

        let mut complete_color_vars = std::collections::HashMap::new();
        let mut all_defined_vars = std::collections::HashSet::new();

        for segment in clean_css.split(['{', '}', ';']) {
            let trimmed = segment.trim();
            if let Some((lhs, rhs)) = trimmed.split_once(':') {
                let var_name = lhs.trim();
                let val = rhs.trim();
                if var_name.starts_with("--")
                    && var_name.len() > 2
                    && var_name[2..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    && !val.is_empty()
                {
                    all_defined_vars.insert(var_name.to_string());
                    let lower = val.to_ascii_lowercase();
                    if lower.starts_with("oklch(")
                        || lower.starts_with("hsl(")
                        || lower.starts_with("hsla(")
                        || lower.starts_with("rgb(")
                        || lower.starts_with("rgba(")
                        || lower.starts_with('#')
                    {
                        complete_color_vars.insert(var_name.to_string(), val.to_string());
                    }
                }
            }
        }

        let mut double_wrapped = Vec::new();
        for fn_name in ["oklch", "hsla", "hsl", "rgba", "rgb"] {
            let pat = format!("{fn_name}(");
            let mut search_slice = clean_css.as_str();
            while let Some(pos) = search_slice.find(&pat) {
                let after_fn = &search_slice[pos + pat.len()..];
                let after_fn_trimmed = after_fn.trim_start();
                if let Some(after_var) = after_fn_trimmed.strip_prefix("var(") {
                    let var_arg = after_var.split([')', ',']).next().unwrap_or("").trim();
                    if let Some(def_val) = complete_color_vars.get(var_arg) {
                        let entry = format!(
                            "`{fn_name}(var({var_arg}))` where `{var_arg}: {def_val}` -> evaluates to invalid `{fn_name}({def_val})`"
                        );
                        if !double_wrapped.contains(&entry) {
                            double_wrapped.push(entry);
                        }
                    }
                }
                search_slice = after_fn;
            }
        }

        if !double_wrapped.is_empty() {
            let file_display = path.display();
            let issues = double_wrapped.join("\n  - ");
            return Err(format!(
                "[CSS Semantic Barrier Rejected]:\n\
                 The proposed edit to '{file_display}' contains double-wrapped CSS color function variable(s):\n\
                   - {issues}\n\
                 Wrapping `var(--token)` inside `oklch(...)` / `hsl(...)` / `rgb(...)` when `--token` already holds a complete color value produces invalid CSS at runtime (causing backgrounds and text colors to silently fail and render transparent/white).\n\
                 CRITICAL: The proposed edit was REJECTED and file contents on disk were preserved untouched.\n\
                 Guidance:\n\
                 - In `@theme` or `@theme inline`, reference the CSS variable directly WITHOUT wrapping: `--color-background: var(--background);`\n\
                 - Keep `:root` / `.dark` variables as complete color values: `--background: oklch(0.14 0.01 260);`"
            ));
        }

        let is_tailwind_v4 = clean_css.contains("@import \"tailwindcss\"")
            || clean_css.contains("@import 'tailwindcss'")
            || clean_css.contains("@theme");
        if is_tailwind_v4
            && clean_css.contains("@apply")
            && clean_css.contains("border-border")
            && !all_defined_vars.contains("--color-border")
            && !all_defined_vars.contains("--border")
        {
            let file_display = path.display();
            return Err(format!(
                "[CSS Semantic Barrier Rejected]:\n\
                 The proposed edit to '{file_display}' uses `@apply border-border` in a Tailwind v4 stylesheet without defining `--color-border` in `@theme` (or `--border`).\n\
                 This causes Vite/Tailwind CSS builds to crash with: `Cannot apply unknown utility class: border-border`.\n\
                 CRITICAL: The proposed edit was REJECTED and file contents on disk were preserved untouched.\n\
                 Guidance:\n\
                 - Either define `--color-border: var(--border);` inside `@theme inline` (and `--border: oklch(...);` in `:root`),\n\
                 - Or remove `@apply border-border;` / replace it with standard border token utilities that you have defined in `@theme`."
            ));
        }

        if is_tailwind_v4 {
            if let Some(offending_rule) = Self::find_unlayered_universal_box_reset(&clean_css) {
                let file_display = path.display();
                return Err(format!(
                    "[CSS Semantic Barrier Rejected]:\n\
                     The proposed edit to '{file_display}' contains an UNLAYERED universal box-model spacing reset (`{offending_rule}`) in a Tailwind CSS v4 stylesheet.\n\
                     In Tailwind CSS v4 (`@import \"tailwindcss\"`), all spacing utilities (`px-*`, `py-*`, `pt-*`, `p-*`, `m-*`, `mx-auto`) are generated inside `@layer utilities`.\n\
                     Per CSS Cascade Layers specification, unlayered rules ALWAYS override `@layer utilities` regardless of specificity — meaning `{offending_rule}` forces every Tailwind padding and margin utility across the entire app to `0px` (collapsing container centering, card padding, button padding, and fixed-navbar top offsets)!\n\
                     CRITICAL: The proposed edit was REJECTED and file contents on disk were preserved untouched.\n\
                     Guidance:\n\
                     - Either REMOVE the manual `* {{ margin: 0; padding: 0; }}` reset completely (Tailwind v4 Preflight already includes `@layer base {{ *, ::after, ::before {{ box-sizing: border-box; margin: 0; padding: 0; }} }}`),\n\
                     - Or wrap your base reset rules inside `@layer base {{ *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }} }}` so `@layer utilities` can override them."
                ));
            }
        }

        Ok(())
    }

    pub fn check_syntax_barrier(
        path: &Path,
        original_content: &str,
        new_content: &str,
    ) -> Result<(), String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        // 0. Pre-write Semantic Barrier: Check for removed brand icons from 'lucide-react' or broken CSS variables
        if ext == "ts"
            || ext == "tsx"
            || ext == "js"
            || ext == "jsx"
            || ext == "mjs"
            || ext == "cjs"
        {
            let new_brands = Self::extract_lucide_brand_imports(new_content);
            if !new_brands.is_empty() {
                let orig_brands = Self::extract_lucide_brand_imports(original_content);
                let introduced: Vec<_> = new_brands
                    .into_iter()
                    .filter(|b| !orig_brands.iter().any(|ob| ob.eq_ignore_ascii_case(b)))
                    .collect();

                if !introduced.is_empty() {
                    let file_display = path.display();
                    let icons_str = introduced.join(", ");
                    return Err(format!(
                        "[AST Import Barrier Rejected]:\n\
                         The proposed edit to '{file_display}' attempts to import brand icon(s) [{icons_str}] from 'lucide-react'.\n\
                         `lucide-react` does NOT export brand/social icons (Github, Twitter, Discord, Linkedin, Google, etc.).\n\
                         Importing them will break TypeScript and bundler builds (TS2305: Module '\"lucide-react\"' has no exported member '{icons_str}').\n\
                         CRITICAL: The proposed edit was REJECTED and file contents on disk were preserved untouched.\n\
                         DO NOT call 'patch_file' assuming your changes were applied! Re-issue 'write_file' with the corrected code.\n\
                         Guidance:\n\
                         - Replace brand icons with inline SVG components (e.g. `<svg viewBox=\"0 0 24 24\" fill=\"currentColor\" className=...><path d=\"...\"/></svg>`).\n\
                         - Or use standard UI icons from 'lucide-react' such as `Globe`, `Share2`, `ExternalLink`, `Code2`, or `Terminal`."
                    ));
                }
            }
        } else if ext == "css" {
            Self::check_css_semantic_barrier(path, new_content)?;
        }

        let lang = match Self::language_for_path(path) {
            Some(l) => l,
            None => return Ok(()),
        };

        // If original content was empty (new file), treat original as clean
        let original_has_error = if original_content.trim().is_empty() {
            false
        } else {
            let mut orig_parser = Parser::new();
            if orig_parser.set_language(&lang).is_ok() {
                orig_parser
                    .parse(original_content, None)
                    .map(|tree| {
                        tree.root_node().has_error()
                            && Self::find_first_error(tree.root_node(), original_content).is_some()
                    })
                    .unwrap_or(false)
            } else {
                false
            }
        };

        // If the original file was already broken, allow write so model can fix syntax
        if original_has_error {
            return Ok(());
        }

        let mut new_parser = Parser::new();
        if new_parser.set_language(&lang).is_err() {
            return Ok(());
        }

        let new_tree = match new_parser.parse(new_content, None) {
            Some(tree) => tree,
            None => return Ok(()),
        };

        let root = new_tree.root_node();
        if !root.has_error() {
            return Ok(());
        }

        let Some(err) = Self::find_first_error(root, new_content) else {
            return Ok(());
        };

        let file_display = path.display();
        let is_new_file = original_content.trim().is_empty();
        let is_jsx_or_tsx = ext == "tsx" || ext == "jsx";

        let (ast_line, ast_col, ast_kind, ast_snippet) =
            (err.line, err.column, err.kind, err.snippet);

        let mut line = ast_line;
        let mut col = ast_col;
        let mut kind = ast_kind.clone();
        let mut snippet = ast_snippet;
        let mut location_suffix = String::new();
        let mut has_edit_imbalance = false;

        if !is_new_file {
            if let Some((first_changed, last_changed, removed_text, added_text)) =
                Self::find_modified_line_span(original_content, new_content)
            {
                let imbalance =
                    Self::detect_edit_imbalance(&removed_text, &added_text, is_jsx_or_tsx);
                if imbalance.is_some() {
                    has_edit_imbalance = true;
                }
                let cascading_outside_diff =
                    ast_line < first_changed || ast_line > last_changed.saturating_add(1);

                if cascading_outside_diff || imbalance.is_some() {
                    let new_lines: Vec<&str> = new_content.lines().collect();
                    let mut target_line = first_changed;
                    for l_num in first_changed..=last_changed {
                        if let Some(l_str) = new_lines.get(l_num.saturating_sub(1)) {
                            if !l_str.trim().is_empty() {
                                target_line = l_num;
                                break;
                            }
                        }
                    }
                    if cascading_outside_diff {
                        if let Some(l_str) = new_lines.get(target_line.saturating_sub(1)) {
                            line = target_line;
                            col = l_str
                                .chars()
                                .take_while(|c| c.is_whitespace())
                                .count()
                                .saturating_add(1);
                            snippet = l_str.trim_end().to_string();
                            location_suffix =
                                format!(" (cascading parser error at line {ast_line}:{ast_col})");
                        }
                    }
                    if let Some(imb_msg) = imbalance {
                        kind = imb_msg;
                    }
                }
            }
        }

        let col_indent = " ".repeat(col.saturating_sub(1));
        let file_status_note = if is_new_file {
            "NOTE: This was a NEW file. Because syntax validation failed, NO file was written to disk.\n\
             Do NOT use 'patch_file'. You must re-issue 'write_file' with the full corrected content."
        } else {
            "The original file on disk was preserved untouched."
        };

        let mut guidance = Vec::new();
        guidance.push(
            "Correct the syntax (check for missing or unclosed braces, quotes, or tags) and retry.",
        );
        if has_edit_imbalance || !location_suffix.is_empty() {
            guidance.push(
                "Each 'patch_file' call validates the entire file immediately. You cannot insert an opening tag or delimiter (e.g. '<div>' or '{') in one 'patch_file' call and its closing counterpart ('</div>' or '}') in a later call — include BOTH the opening and closing tags/delimiters in the same edit (or use 'write_file').",
            );
        }
        if is_jsx_or_tsx {
            guidance.push("JSX Rule: Dynamic component tags like '<categories[i].icon />' or '<items[0] />' are invalid JSX syntax. Assign the component to a capitalized local variable first: 'const Icon = categories[i].icon;' then render '<Icon className=... />'.");
        }
        let guidance_str = guidance.join("\n- ");

        let diagnostic = format!(
            "[AST Syntax Barrier Rejected]:\n\
             The proposed edit to '{file_display}' introduces a syntax error at line {line}:{col}{location_suffix}:\n\
             ------------------------------------------------------------\n\
             {line:>4} | {snippet}\n\
                  | {col_indent}^ {kind}\n\
             ------------------------------------------------------------\n\
             {file_status_note}\n\
             Guidance:\n- {guidance_str}"
        );

        Err(diagnostic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_valid_rust_code_passes() {
        let path = PathBuf::from("src/main.rs");
        let orig = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello, world!\");\n}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_syntax_breaking_rust_edit_rejected() {
        let path = PathBuf::from("src/lib.rs");
        let orig = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        // Missing closing brace
        let new = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[AST Syntax Barrier Rejected]"));
        assert!(msg.contains("src/lib.rs"));
        assert!(msg.contains("at line"));
    }

    #[test]
    fn test_syntax_repair_of_broken_original_allowed() {
        let path = PathBuf::from("src/broken.rs");
        // Original was broken
        let orig = "fn broken( {";
        // New is fixed
        let new = "fn broken() {}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_unsupported_language_transparently_passed() {
        let path = PathBuf::from("README.md");
        let orig = "# Hello";
        let new = "# Hello\n```broken { [";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_python_syntax_barrier() {
        let path = PathBuf::from("script.py");
        let orig = "def greet(name):\n    return f'Hello {name}'\n";
        // Broken python syntax
        let new = "def greet(name:\n    return f'Hello {name}'\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[AST Syntax Barrier Rejected]"));
    }

    #[test]
    fn test_lucide_brand_icon_barrier_rejects() {
        let path = PathBuf::from("src/components/Footer.tsx");
        let orig = "";
        let new = "import React from 'react';\nimport { Github, Twitter, ArrowRight } from 'lucide-react';\nexport function Footer() { return <footer><Github /></footer>; }\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[AST Import Barrier Rejected]"));
        assert!(msg.contains("Github"));
        assert!(msg.contains("Twitter"));
        assert!(msg.contains("lucide-react"));
    }

    #[test]
    fn test_existing_lucide_brand_icon_allowed() {
        let path = PathBuf::from("src/components/Footer.tsx");
        let orig = "import { Github } from 'lucide-react';\n";
        let new = "import { Github, ArrowRight } from 'lucide-react';\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_lucide_brand_icon_variants_rejected() {
        let path = PathBuf::from("src/components/Footer.tsx");
        let orig = "";
        let new = "import React from 'react';\nimport { GitHubAction, TwitterIcon, ArrowRight } from 'lucide-react';\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[AST Import Barrier Rejected]"));
        assert!(msg.contains("GitHubAction"));
        assert!(msg.contains("TwitterIcon"));
    }

    #[test]
    fn test_css_double_wrapped_oklch_rejected() {
        let path = PathBuf::from("src/index.css");
        let orig = "";
        let new = "@import \"tailwindcss\";\n@theme {\n  --color-background: oklch(var(--bg-base));\n}\n:root {\n  --bg-base: oklch(0.05 0 0);\n}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[CSS Semantic Barrier Rejected]"));
        assert!(msg.contains("oklch(var(--bg-base))"));
    }

    #[test]
    fn test_css_missing_border_border_rejected() {
        let path = PathBuf::from("src/index.css");
        let orig = "";
        let new = "@import \"tailwindcss\";\n@theme {\n  --color-background: var(--bg-base);\n}\n@layer base {\n  * {\n    @apply border-border;\n  }\n}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[CSS Semantic Barrier Rejected]"));
        assert!(msg.contains("border-border"));
    }

    #[test]
    fn test_css_clean_tailwind_v4_passes() {
        let path = PathBuf::from("src/index.css");
        let orig = "";
        let new = "@import \"tailwindcss\";\n@theme inline {\n  --color-background: var(--background);\n  --color-border: var(--border);\n}\n:root {\n  --background: oklch(0.05 0 0);\n  --border: oklch(0.2 0 0);\n}\n@layer base {\n  * {\n    @apply border-border;\n  }\n}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_tailwind_v4_unlayered_universal_reset_rejected_and_layered_passes() {
        let path = PathBuf::from("src/index.css");
        let orig = "";
        let unlayered = "@import \"tailwindcss\";\n:root {\n  --bg-primary: #06090f;\n}\n*, *::before, *::after {\n  box-sizing: border-box;\n  margin: 0;\n  padding: 0;\n}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, unlayered);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[CSS Semantic Barrier Rejected]"));
        assert!(msg.contains("UNLAYERED universal box-model spacing reset"));

        let layered = "@import \"tailwindcss\";\n:root {\n  --bg-primary: #06090f;\n}\n@layer base {\n  *, *::before, *::after {\n    box-sizing: border-box;\n    margin: 0;\n    padding: 0;\n  }\n}\n";
        assert!(SyntaxGuard::check_syntax_barrier(&path, orig, layered).is_ok());
    }

    #[test]
    fn test_tsx_unclosed_jsx_wrapper_patch_pinpoints_modified_line_and_guidance() {
        let path = PathBuf::from("src/components/Swarm.tsx");
        let orig = "export function Swarm() {\n  return (\n    <section id=\"swarm\">\n      <div className=\"outer\">\n        <svg viewBox=\"0 0 100 100\"><rect width=\"10\" height=\"10\" /></svg>\n      </div>\n    </section>\n  );\n}\n";
        // Simulate inserting an opening `<div className="overflow-x-auto">` at line 5 without its closing `</div>`
        let patched = "export function Swarm() {\n  return (\n    <section id=\"swarm\">\n      <div className=\"outer\">\n        <div className=\"overflow-x-auto\">\n        <svg viewBox=\"0 0 100 100\"><rect width=\"10\" height=\"10\" /></svg>\n      </div>\n    </section>\n  );\n}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, patched);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("at line 5:9"), "msg was: {msg}");
        assert!(
            msg.contains("unclosed '<div>' introduced here without a matching '</div>'"),
            "msg was: {msg}"
        );
        assert!(
            msg.contains("include BOTH the opening and closing tags/delimiters in the same edit"),
            "msg was: {msg}"
        );
    }

    #[test]
    fn test_tsx_bare_ampersand_in_jsx_text_passes_while_unclosed_tag_fails() {
        let path = PathBuf::from("src/components/terminal/BrowserHUD.tsx");
        let valid_tsx = "export function BrowserHUD() {\n  return (\n    <div>\n      <Label className=\"text-ink-faint\">Console & network</Label>\n      <span>R&D + Q&A</span>\n    </div>\n  );\n}\n";
        let syntax_res = SyntaxGuard::check_syntax(&path, valid_tsx);
        assert!(
            syntax_res.is_none(),
            "unexpected syntax_res: {syntax_res:?}"
        );
        assert!(SyntaxGuard::check_syntax_barrier(&path, "", valid_tsx).is_ok());

        let broken_tsx = "export function BrowserHUD() {\n  return (\n    <div>\n      <Label className=\"text-ink-faint\">Console & network\n    </div>\n  );\n}\n";
        assert!(SyntaxGuard::check_syntax(&path, broken_tsx).is_some());
        assert!(SyntaxGuard::check_syntax_barrier(&path, valid_tsx, broken_tsx).is_err());
    }
}
