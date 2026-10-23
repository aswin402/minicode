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

    /// Recursively walks the AST to locate the first ERROR or MISSING node.
    fn find_first_error(node: Node, code: &str) -> Option<SyntaxErrorDetail> {
        if node.is_error() || node.is_missing() {
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
                    .map(|tree| tree.root_node().has_error())
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

        let file_display = path.display();

        let err_detail = Self::find_first_error(root, new_content);
        let (line, col, kind, snippet) = match err_detail {
            Some(err) => (err.line, err.column, err.kind, err.snippet),
            None => (1, 1, "unparseable syntax".to_string(), String::new()),
        };

        let col_indent = " ".repeat(col.saturating_sub(1));
        let is_new_file = original_content.trim().is_empty();
        let file_status_note = if is_new_file {
            "NOTE: This was a NEW file. Because syntax validation failed, NO file was written to disk.\n\
             Do NOT use 'patch_file'. You must re-issue 'write_file' with the full corrected content."
        } else {
            "The original file on disk was preserved untouched."
        };

        let is_jsx_or_tsx = ext == "tsx" || ext == "jsx";

        let mut guidance = Vec::new();
        guidance.push(
            "Correct the syntax (check for missing or unclosed braces, quotes, or tags) and retry.",
        );
        if is_jsx_or_tsx {
            guidance.push("JSX Rule: Dynamic component tags like '<categories[i].icon />' or '<items[0] />' are invalid JSX syntax. Assign the component to a capitalized local variable first: 'const Icon = categories[i].icon;' then render '<Icon className=... />'.");
        }
        let guidance_str = guidance.join("\n- ");

        let diagnostic = format!(
            "[AST Syntax Barrier Rejected]:\n\
             The proposed edit to '{file_display}' introduces a syntax error at line {line}:{col}:\n\
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
}
