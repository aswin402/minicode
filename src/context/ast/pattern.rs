//! Structural AST pattern matching & rewriting engine inspired by `ast-grep/ast-grep`.
//!
//! Key capabilities ported from `ast-grep`:
//! 1. **Expando Metavariable Preprocessing (`$VAR`, `$_`, `$$$ARGS`, `$$$`)**:
//!    Rewrites metavariables into valid language identifiers before parsing with Tree-sitter,
//!    auto-wrapping expression/statement snippets so patterns parse cleanly into [`PatternNode`] trees.
//! 2. **Smart Tree-to-Tree Structural Matching & [`MetaVarEnv`]**:
//!    Matches single AST nodes (`$VAR`), unnamed wildcards (`$_`), and variadic multi-node sequences
//!    (`$$$ARGS` / `$$$`), enforcing structural AST equality when a `$VAR` appears multiple times
//!    in a pattern (e.g. `$X == $X`).
//! 3. **Relational AST Constraints ([`AstGrepFilter`])**:
//!    Supports `inside`, `not_inside`, and `has` ancestor/descendant structural constraints.
//! 4. **Fixed-Literal Trigram Pre-Filtering ([`PatternNode::fixed_string`])**:
//!    Extracts the longest fixed terminal literal from the pattern and queries [`TrigramIndex`]
//!    to skip non-matching files before running Tree-sitter.
//! 5. **Indentation-Preserving Template Rewriter (`ast_grep_rewrite`)**:
//!    Substitutes captured `$VAR` and `$$$ARGS` bindings with de-indent/re-indent alignment,
//!    splices non-overlapping matches in reverse byte order, and validates syntax via [`SyntaxGuard`].

use crate::constants::MAX_SEARCH_RESULTS;
use crate::context::syntax_guard::SyntaxGuard;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

const EXPANDO_SINGLE_PREFIX: &str = "__sg_var_";
const EXPANDO_MULTI_PREFIX: &str = "__sg_multi_";
const EXPANDO_WILD_SINGLE: &str = "__sg_wild__";
const EXPANDO_WILD_MULTI: &str = "__sg_multi_wild__";
const EXPANDO_SUFFIX: &str = "__";

/// Metavariable capture specification (`ast-grep` `MetaVariable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaVarKind {
    /// `$NAME` — captures a single AST node.
    Single(String),
    /// `$_` — matches any single AST node without capturing.
    Dropped,
    /// `$$$NAME` — captures 0 or more sibling AST nodes.
    Multi(String),
    /// `$$$` or `$$$_` — matches 0 or more sibling AST nodes without capturing.
    MultiDropped,
}

/// Structural pattern tree node (`ast-grep` `PatternNode`).
#[derive(Debug, Clone)]
pub enum PatternNode {
    MetaVar(MetaVarKind),
    Terminal {
        kind: String,
        text: String,
        is_named: bool,
    },
    Internal {
        kind: String,
        children: Vec<PatternNode>,
    },
}

impl PatternNode {
    /// Extracts the longest fixed terminal literal from the pattern tree
    /// (`ast-grep` `PatternNode::fixed_string` for trigram pre-filtering).
    pub fn fixed_string(&self) -> &str {
        match self {
            PatternNode::MetaVar(_) => "",
            PatternNode::Terminal { text, .. } => text.as_str(),
            PatternNode::Internal { children, .. } => children
                .iter()
                .map(|c| c.fixed_string())
                .fold("", |longest, curr| {
                    if longest.len() >= curr.len() {
                        longest
                    } else {
                        curr
                    }
                }),
        }
    }

    fn is_variadic(&self) -> bool {
        matches!(
            self,
            PatternNode::MetaVar(MetaVarKind::Multi(_) | MetaVarKind::MultiDropped)
        )
    }

    fn is_optional_punctuation(&self) -> bool {
        matches!(
            self,
            PatternNode::Terminal {
                is_named: false,
                text,
                ..
            } if text == "," || text == ";"
        )
    }
}

/// Captured metavariable binding span in source text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapturedVar {
    pub name: String,
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
}

/// Dictionary of metavariable bindings accumulated during structural matching (`ast-grep` `MetaVarEnv`).
#[derive(Debug, Clone, Default)]
pub struct MetaVarEnv<'tree> {
    single_nodes: HashMap<String, Node<'tree>>,
    multi_nodes: HashMap<String, Vec<Node<'tree>>>,
}

impl<'tree> MetaVarEnv<'tree> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_single(&mut self, name: &str, candidate: Node<'tree>, source: &str) -> bool {
        if let Some(&existing) = self.single_nodes.get(name) {
            return nodes_structurally_equal(existing, candidate, source);
        }
        self.single_nodes.insert(name.to_string(), candidate);
        true
    }

    pub fn bind_multi(&mut self, name: &str, candidates: Vec<Node<'tree>>, source: &str) -> bool {
        if let Some(existing) = self.multi_nodes.get(name) {
            let existing_named: Vec<Node<'tree>> =
                existing.iter().copied().filter(|n| n.is_named()).collect();
            let cand_named: Vec<Node<'tree>> = candidates
                .iter()
                .copied()
                .filter(|n| n.is_named())
                .collect();
            if existing_named.len() != cand_named.len() {
                return false;
            }
            for (a, b) in existing_named.into_iter().zip(cand_named.into_iter()) {
                if !nodes_structurally_equal(a, b, source) {
                    return false;
                }
            }
            return true;
        }
        self.multi_nodes.insert(name.to_string(), candidates);
        true
    }

    pub fn to_captured_map(&self, source: &str) -> HashMap<String, CapturedVar> {
        let mut out = HashMap::new();
        for (name, node) in &self.single_nodes {
            let start_b = node.start_byte();
            let end_b = node.end_byte();
            let text = source.get(start_b..end_b).unwrap_or("").to_string();
            out.insert(
                name.clone(),
                CapturedVar {
                    name: name.clone(),
                    text,
                    start_byte: start_b,
                    end_byte: end_b,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                },
            );
        }
        for (name, nodes) in &self.multi_nodes {
            if nodes.is_empty() {
                out.insert(
                    name.clone(),
                    CapturedVar {
                        name: name.clone(),
                        text: String::new(),
                        start_byte: 0,
                        end_byte: 0,
                        start_line: 0,
                        end_line: 0,
                    },
                );
            } else {
                // Trim leading/trailing comma tokens from multi-capture slice when bounded by named nodes
                let start_idx = nodes
                    .iter()
                    .position(|n| source.get(n.start_byte()..n.end_byte()).unwrap_or("") != ",")
                    .unwrap_or(0);
                let end_idx = nodes
                    .iter()
                    .rposition(|n| source.get(n.start_byte()..n.end_byte()).unwrap_or("") != ",")
                    .unwrap_or(nodes.len() - 1);
                let slice = if start_idx <= end_idx {
                    &nodes[start_idx..=end_idx]
                } else {
                    &nodes[..]
                };
                let first = slice[0];
                let last = slice[slice.len() - 1];
                let start_b = first.start_byte();
                let end_b = last.end_byte();
                let text = source.get(start_b..end_b).unwrap_or("").to_string();
                out.insert(
                    name.clone(),
                    CapturedVar {
                        name: name.clone(),
                        text,
                        start_byte: start_b,
                        end_byte: end_b,
                        start_line: first.start_position().row + 1,
                        end_line: last.end_position().row + 1,
                    },
                );
            }
        }
        out
    }
}

/// Checks whether two AST nodes are structurally identical, ignoring comments and whitespace differences
/// (`ast-grep` `does_node_match_exactly`).
pub fn nodes_structurally_equal(a: Node<'_>, b: Node<'_>, source: &str) -> bool {
    if a.kind() != b.kind() {
        return false;
    }
    let a_children: Vec<Node<'_>> = non_comment_children(a);
    let b_children: Vec<Node<'_>> = non_comment_children(b);
    if a_children.is_empty() && b_children.is_empty() {
        let a_text = source.get(a.start_byte()..a.end_byte()).unwrap_or("");
        let b_text = source.get(b.start_byte()..b.end_byte()).unwrap_or("");
        return a_text == b_text;
    }
    if a_children.len() != b_children.len() {
        return false;
    }
    a_children
        .into_iter()
        .zip(b_children)
        .all(|(ca, cb)| nodes_structurally_equal(ca, cb, source))
}

fn is_comment_node(node: Node<'_>) -> bool {
    node.is_extra() || node.kind().contains("comment")
}

fn non_comment_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| !is_comment_node(*c))
        .collect()
}

/// Rewrites `$VAR`, `$_`, `$$$VAR`, and `$$$` metavariables (outside string literals) into valid
/// language identifiers so Tree-sitter parses pattern snippets without syntax errors.
pub fn preprocess_pattern(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 32);
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let mut in_quote: Option<char> = None;

    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = in_quote {
            out.push(ch);
            if ch == '\\' && i + 1 < chars.len() {
                i += 1;
                out.push(chars[i]);
            } else if ch == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
            out.push(ch);
            i += 1;
            continue;
        }

        if ch == '$' {
            // Check for $$$ (variadic multi-metavar)
            if i + 2 < chars.len() && chars[i + 1] == '$' && chars[i + 2] == '$' {
                i += 3;
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                if name.is_empty() || name.starts_with('_') {
                    out.push_str(EXPANDO_WILD_MULTI);
                } else {
                    out.push_str(EXPANDO_MULTI_PREFIX);
                    out.push_str(&name);
                    out.push_str(EXPANDO_SUFFIX);
                }
                continue;
            }

            // Check for $_ or $NAME
            if i + 1 < chars.len() && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_') {
                i += 1;
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                if name == "_" {
                    out.push_str(EXPANDO_WILD_SINGLE);
                } else {
                    out.push_str(EXPANDO_SINGLE_PREFIX);
                    out.push_str(&name);
                    out.push_str(EXPANDO_SUFFIX);
                }
                continue;
            }
        }

        out.push(ch);
        i += 1;
    }

    out
}

fn extract_expando_metavar(text: &str) -> Option<MetaVarKind> {
    let trimmed = text.trim();
    if trimmed == EXPANDO_WILD_SINGLE {
        return Some(MetaVarKind::Dropped);
    }
    if trimmed == EXPANDO_WILD_MULTI {
        return Some(MetaVarKind::MultiDropped);
    }
    if let Some(rest) = trimmed.strip_prefix(EXPANDO_MULTI_PREFIX) {
        if let Some(name) = rest.strip_suffix(EXPANDO_SUFFIX) {
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Some(MetaVarKind::Multi(name.to_string()));
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix(EXPANDO_SINGLE_PREFIX) {
        if let Some(name) = rest.strip_suffix(EXPANDO_SUFFIX) {
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Some(MetaVarKind::Single(name.to_string()));
            }
        }
    }
    None
}

/// Compiled structural AST pattern ready to match against a Tree-sitter syntax tree.
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    pub root: PatternNode,
    pub raw_pattern: String,
}

impl CompiledPattern {
    /// Compiles a pattern snippet for the given file extension (`rs`, `py`, `ts`, `tsx`, `js`, `jsx`).
    pub fn compile(pattern_str: &str, ext: &str) -> Result<Self> {
        let trimmed_pat = pattern_str.trim();
        if trimmed_pat.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "ast_grep".to_string(),
                reason: "AST pattern cannot be empty".to_string(),
            }
            .into());
        }

        let lang = SyntaxGuard::language_for_extension(ext).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "ast_grep".to_string(),
                reason: format!("Unsupported language extension for AST pattern: '{}'", ext),
            }
        })?;

        let mut parser = Parser::new();
        parser
            .set_language(&lang)
            .map_err(|e| ToolError::CommandExec(format!("Tree-sitter parser error: {}", e)))?;

        let expando = preprocess_pattern(trimmed_pat);
        let has_trailing_semi = trimmed_pat.ends_with(';');

        // 1. Try parsing the pattern snippet directly
        if let Some(tree) = parser.parse(&expando, None) {
            let root = tree.root_node();
            if !has_non_metavar_error(root, &expando) {
                if let Some(pattern_node) =
                    extract_pattern_root_from_tree(root, &expando, has_trailing_semi)
                {
                    return Ok(Self {
                        root: pattern_node,
                        raw_pattern: trimmed_pat.to_string(),
                    });
                }
            }
        }

        // 2. Fallback: wrap expression/statement snippet inside a dummy function body (needed for Rust expressions/statements)
        let wrapped_candidates: Vec<String> = match ext {
            "rs" => vec![
                format!("fn __sg_wrap__() {{\n{}\n}}", expando),
                format!("impl __SgDummy__ {{\n{}\n}}", expando),
            ],
            "ts" | "tsx" | "js" | "jsx" => vec![
                format!("function __sg_wrap__() {{\n{}\n}}", expando),
                format!("class __SgDummy__ {{\n{}\n}}", expando),
            ],
            "py" => vec![format!("def __sg_wrap__():\n    {}\n", expando)],
            _ => Vec::new(),
        };

        for wrapped in &wrapped_candidates {
            if let Some(tree) = parser.parse(wrapped, None) {
                if let Some(inner_node) = find_wrapped_body_child(tree.root_node()) {
                    let unwrapped = unwrap_stmt_if_needed(inner_node, has_trailing_semi);
                    return Ok(Self {
                        root: convert_node_to_pattern(unwrapped, wrapped),
                        raw_pattern: trimmed_pat.to_string(),
                    });
                }
            }
        }

        // 3. Best-effort fallback on direct parse even if partial grammar error exists (e.g. `fn $NAME($$$PARAMS)`)
        if let Some(tree) = parser.parse(&expando, None) {
            if let Some(pattern_node) =
                extract_pattern_root_from_tree(tree.root_node(), &expando, has_trailing_semi)
            {
                return Ok(Self {
                    root: pattern_node,
                    raw_pattern: trimmed_pat.to_string(),
                });
            }
        }

        Err(ToolError::InvalidArguments {
            name: "ast_grep".to_string(),
            reason: format!(
                "Could not parse AST pattern '{}' for language '{}'",
                trimmed_pat, ext
            ),
        }
        .into())
    }

    /// Attempts to match this pattern against `candidate` and returns the bound [`MetaVarEnv`] on match.
    pub fn matches_node<'tree>(
        &self,
        candidate: Node<'tree>,
        source: &str,
    ) -> Option<MetaVarEnv<'tree>> {
        if is_comment_node(candidate) {
            return None;
        }
        let mut env = MetaVarEnv::new();
        if match_one_node(&self.root, candidate, source, &mut env) {
            Some(env)
        } else {
            None
        }
    }
}

fn has_non_metavar_error(node: Node<'_>, source: &str) -> bool {
    let text = source.get(node.start_byte()..node.end_byte()).unwrap_or("");
    if extract_expando_metavar(text).is_some() {
        return false;
    }
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_non_metavar_error(child, source) {
            return true;
        }
    }
    false
}

fn extract_pattern_root_from_tree(
    root: Node<'_>,
    source: &str,
    has_trailing_semi: bool,
) -> Option<PatternNode> {
    let children = non_comment_children(root);
    let first = *children.first()?;
    let target = unwrap_stmt_if_needed(first, has_trailing_semi);
    Some(convert_node_to_pattern(target, source))
}

fn unwrap_stmt_if_needed<'a>(node: Node<'a>, has_trailing_semi: bool) -> Node<'a> {
    if !has_trailing_semi && node.kind() == "expression_statement" {
        let children = non_comment_children(node);
        if let Some(&first_named) = children.iter().find(|c| c.is_named()) {
            return first_named;
        }
    }
    node
}

fn find_wrapped_body_child(root: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if matches!(
            child.kind(),
            "block" | "statement_block" | "declaration_list" | "class_body"
        ) {
            let inner: Vec<Node<'_>> = non_comment_children(child)
                .into_iter()
                .filter(|c| c.is_named())
                .collect();
            if let Some(&first) = inner.first() {
                return Some(first);
            }
        }
        if let Some(found) = find_wrapped_body_child(child) {
            return Some(found);
        }
    }
    None
}

fn convert_node_to_pattern(node: Node<'_>, source: &str) -> PatternNode {
    let text = source.get(node.start_byte()..node.end_byte()).unwrap_or("");
    if let Some(mv) = extract_expando_metavar(text) {
        return PatternNode::MetaVar(mv);
    }

    let children = non_comment_children(node);
    if children.is_empty() {
        return PatternNode::Terminal {
            kind: node.kind().to_string(),
            text: text.to_string(),
            is_named: node.is_named(),
        };
    }

    // If a wrapper node has a single child with identical byte range and that child is a metavar, unwrap it
    if children.len() == 1 {
        let only = children[0];
        let only_text = source.get(only.start_byte()..only.end_byte()).unwrap_or("");
        if let Some(mv) = extract_expando_metavar(only_text) {
            return PatternNode::MetaVar(mv);
        }
    }

    let pattern_children = children
        .into_iter()
        .filter(|c| !c.is_missing())
        .map(|c| convert_node_to_pattern(c, source))
        .collect();

    PatternNode::Internal {
        kind: node.kind().to_string(),
        children: pattern_children,
    }
}

fn is_optional_candidate_modifier(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "visibility_modifier" | "function_modifiers" | "mutable_specifier"
    )
}

fn match_one_node<'tree>(
    goal: &PatternNode,
    candidate: Node<'tree>,
    source: &str,
    env: &mut MetaVarEnv<'tree>,
) -> bool {
    match goal {
        PatternNode::MetaVar(mv) => {
            if is_comment_node(candidate) {
                return false;
            }
            match mv {
                MetaVarKind::Single(name) => env.bind_single(name, candidate, source),
                MetaVarKind::Dropped => true,
                MetaVarKind::Multi(name) => env.bind_multi(name, vec![candidate], source),
                MetaVarKind::MultiDropped => true,
            }
        }
        PatternNode::Terminal {
            kind,
            text,
            is_named,
        } => {
            let cand_text = source
                .get(candidate.start_byte()..candidate.end_byte())
                .unwrap_or("");
            if *is_named {
                candidate.kind() == kind && cand_text == text
            } else {
                cand_text == text || (candidate.kind() == kind && !candidate.is_named())
            }
        }
        PatternNode::Internal { kind, children } => {
            if candidate.kind() != kind {
                return false;
            }
            let cand_children = non_comment_children(candidate);
            match_children_slice(children, &cand_children, source, env)
        }
    }
}

fn match_children_slice<'tree>(
    goals: &[PatternNode],
    cands: &[Node<'tree>],
    source: &str,
    env: &mut MetaVarEnv<'tree>,
) -> bool {
    if goals.is_empty() {
        return cands
            .iter()
            .all(|c| !c.is_named() || is_comment_node(*c) || is_optional_candidate_modifier(*c));
    }

    // Handle variadic `$$$NAME` or `$$$`
    if let PatternNode::MetaVar(mv @ (MetaVarKind::Multi(_) | MetaVarKind::MultiDropped)) =
        &goals[0]
    {
        let mut rest_goals = &goals[1..];
        // Skip optional separator comma immediately following $$$ in pattern
        if let Some(first_rest) = rest_goals.first() {
            if first_rest.is_optional_punctuation() {
                rest_goals = &rest_goals[1..];
            }
        }

        for split_idx in 0..=cands.len() {
            let mut probe_env = env.clone();
            let bound_ok = match mv {
                MetaVarKind::Multi(name) => {
                    probe_env.bind_multi(name, cands[..split_idx].to_vec(), source)
                }
                _ => true,
            };
            if bound_ok
                && match_children_slice(rest_goals, &cands[split_idx..], source, &mut probe_env)
            {
                *env = probe_env;
                return true;
            }
        }
        return false;
    }

    // If next goal is optional punctuation (`,` or `;`) and next candidate is a variadic or closing delimiter
    if goals[0].is_optional_punctuation() {
        if cands.is_empty() {
            return match_children_slice(&goals[1..], cands, source, env);
        }
        let mut probe_env = env.clone();
        if match_one_node(&goals[0], cands[0], source, &mut probe_env)
            && match_children_slice(&goals[1..], &cands[1..], source, &mut probe_env)
        {
            *env = probe_env;
            return true;
        }
        // Allow skipping optional comma/semicolon in goal (e.g. before empty `$$$REST`)
        if goals.get(1).is_some_and(|g| g.is_variadic()) {
            return match_children_slice(&goals[1..], cands, source, env);
        }
    }

    if cands.is_empty() {
        return false;
    }

    // Try direct match of `goals[0]` against `cands[0]`
    {
        let mut probe_env = env.clone();
        if match_one_node(&goals[0], cands[0], source, &mut probe_env)
            && match_children_slice(&goals[1..], &cands[1..], source, &mut probe_env)
        {
            *env = probe_env;
            return true;
        }
    }

    // In Smart mode, skip unnamed punctuation or optional modifiers on candidate when goal expects a named/structural node
    if !cands[0].is_named() || is_optional_candidate_modifier(cands[0]) {
        return match_children_slice(goals, &cands[1..], source, env);
    }

    false
}

/// Relational & file-scope constraints for structural AST search and rewrite (`ast-grep` relational rules).
#[derive(Debug, Clone, Default)]
pub struct AstGrepFilter {
    /// Require matched node to be nested inside an ancestor matching this AST kind or pattern.
    pub inside: Option<String>,
    /// Exclude matches nested inside an ancestor matching this AST kind or pattern.
    pub not_inside: Option<String>,
    /// Require matched node to contain a descendant matching this AST kind or pattern.
    pub has: Option<String>,
    /// Optional glob or relative file path filter.
    pub file_pattern: Option<String>,
}

/// A single structural AST match in a workspace file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AstGrepMatch {
    pub file_path: String,
    pub node_kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub matched_text: String,
    pub enclosing_symbol: Option<String>,
    pub metavars: HashMap<String, CapturedVar>,
}

/// Result of a structural AST rewrite operation (`ast_grep_rewrite`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AstGrepRewriteReport {
    pub pattern: String,
    pub replacement: String,
    pub dry_run: bool,
    pub total_matches: usize,
    pub files_modified: Vec<String>,
    pub diff_preview: String,
}

impl AstGrepRewriteReport {
    pub fn format_markdown(&self) -> String {
        if self.total_matches == 0 {
            return format!(
                "ℹ No structural AST matches found for pattern `{}`.",
                self.pattern
            );
        }
        let mode_badge = if self.dry_run {
            "DRY RUN (no files modified on disk)"
        } else {
            "APPLIED (SyntaxGuard verified)"
        };
        format!(
            "🌳 **AST Structural Rewrite** [{}]\n\
             • **Pattern:** `{}`\n\
             • **Replacement:** `{}`\n\
             • **Matches Rewritten:** {} across {} file(s)\n\n\
             ```diff\n{}\n```",
            mode_badge,
            self.pattern,
            self.replacement,
            self.total_matches,
            self.files_modified.len(),
            self.diff_preview.trim()
        )
    }
}

fn node_matches_relational_selector(
    node: Node<'_>,
    selector: &str,
    compiled_opt: Option<&CompiledPattern>,
    source: &str,
) -> bool {
    let sel = selector.trim();
    if sel.is_empty() {
        return true;
    }
    if node.kind().eq_ignore_ascii_case(sel) {
        return true;
    }
    if let Some(compiled) = compiled_opt {
        if compiled.matches_node(node, source).is_some() {
            return true;
        }
    }
    // Check declaration header line (e.g. `mod tests`, `#[cfg(test)]`, `impl Foo`)
    let snippet = source.get(node.start_byte()..node.end_byte()).unwrap_or("");
    let first_line = snippet.lines().next().unwrap_or("");
    if matches!(
        node.kind(),
        "mod_item"
            | "function_item"
            | "impl_item"
            | "struct_item"
            | "trait_item"
            | "attribute_item"
            | "function_definition"
            | "class_definition"
            | "function_declaration"
            | "class_declaration"
            | "method_definition"
    ) && first_line.contains(sel)
    {
        return true;
    }
    false
}

fn check_relational_constraints(
    node: Node<'_>,
    source: &str,
    filter: &AstGrepFilter,
    inside_compiled: Option<&CompiledPattern>,
    not_inside_compiled: Option<&CompiledPattern>,
    has_compiled: Option<&CompiledPattern>,
) -> bool {
    if let Some(ref inside_sel) = filter.inside {
        let mut curr = node.parent();
        let mut found = false;
        while let Some(anc) = curr {
            if node_matches_relational_selector(anc, inside_sel, inside_compiled, source) {
                found = true;
                break;
            }
            curr = anc.parent();
        }
        if !found {
            return false;
        }
    }

    if let Some(ref not_inside_sel) = filter.not_inside {
        let mut curr = node.parent();
        while let Some(anc) = curr {
            if node_matches_relational_selector(anc, not_inside_sel, not_inside_compiled, source) {
                return false;
            }
            curr = anc.parent();
        }
    }

    if let Some(ref has_sel) = filter.has {
        if !subtree_has_descendant(node, has_sel, has_compiled, source, true) {
            return false;
        }
    }

    true
}

fn subtree_has_descendant(
    node: Node<'_>,
    selector: &str,
    compiled_opt: Option<&CompiledPattern>,
    source: &str,
    is_root: bool,
) -> bool {
    if !is_root && node_matches_relational_selector(node, selector, compiled_opt, source) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if subtree_has_descendant(child, selector, compiled_opt, source, false) {
            return true;
        }
    }
    false
}

fn find_enclosing_symbol_label(node: Node<'_>, source: &str) -> Option<String> {
    let mut curr = node.parent();
    while let Some(anc) = curr {
        let kind = anc.kind();
        if matches!(
            kind,
            "function_item"
                | "struct_item"
                | "impl_item"
                | "trait_item"
                | "function_definition"
                | "class_definition"
                | "function_declaration"
                | "class_declaration"
                | "method_definition"
        ) {
            let name = anc
                .child_by_field_name("name")
                .or_else(|| anc.child_by_field_name("type"))
                .and_then(|n| source.get(n.start_byte()..n.end_byte()))
                .unwrap_or(kind);
            return Some(format!(
                "{} {} (L{}-L{})",
                kind,
                name,
                anc.start_position().row + 1,
                anc.end_position().row + 1
            ));
        }
        curr = anc.parent();
    }
    None
}

/// Searches a single parsed file's AST for all occurrences of `pattern` satisfying `filter`.
pub fn find_matches_in_source(
    rel_path: &str,
    ext: &str,
    source: &str,
    pattern_str: &str,
    filter: &AstGrepFilter,
    max_matches: usize,
) -> Result<Vec<AstGrepMatch>> {
    let compiled = CompiledPattern::compile(pattern_str, ext)?;
    let fixed_lit = compiled.root.fixed_string();
    if !fixed_lit.is_empty() && !source.contains(fixed_lit) {
        return Ok(Vec::new());
    }

    let lang =
        SyntaxGuard::language_for_extension(ext).ok_or_else(|| ToolError::InvalidArguments {
            name: "ast_grep".to_string(),
            reason: format!("Unsupported extension '{}'", ext),
        })?;
    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .map_err(|e| ToolError::CommandExec(format!("Tree-sitter parser error: {}", e)))?;

    let Some(tree) = parser.parse(source, None) else {
        return Ok(Vec::new());
    };

    let inside_compiled = filter
        .inside
        .as_deref()
        .and_then(|s| CompiledPattern::compile(s, ext).ok());
    let not_inside_compiled = filter
        .not_inside
        .as_deref()
        .and_then(|s| CompiledPattern::compile(s, ext).ok());
    let has_compiled = filter
        .has
        .as_deref()
        .and_then(|s| CompiledPattern::compile(s, ext).ok());

    let mut matches = Vec::new();
    collect_matches_dfs(
        tree.root_node(),
        rel_path,
        source,
        &compiled,
        filter,
        inside_compiled.as_ref(),
        not_inside_compiled.as_ref(),
        has_compiled.as_ref(),
        max_matches,
        &mut matches,
    );
    Ok(matches)
}

#[allow(clippy::too_many_arguments)]
fn collect_matches_dfs<'tree>(
    node: Node<'tree>,
    rel_path: &str,
    source: &str,
    compiled: &CompiledPattern,
    filter: &AstGrepFilter,
    inside_compiled: Option<&CompiledPattern>,
    not_inside_compiled: Option<&CompiledPattern>,
    has_compiled: Option<&CompiledPattern>,
    max_matches: usize,
    out: &mut Vec<AstGrepMatch>,
) {
    if out.len() >= max_matches {
        return;
    }

    if let Some(env) = compiled.matches_node(node, source) {
        if check_relational_constraints(
            node,
            source,
            filter,
            inside_compiled,
            not_inside_compiled,
            has_compiled,
        ) {
            let start_b = node.start_byte();
            let end_b = node.end_byte();
            let matched_text = source.get(start_b..end_b).unwrap_or("").to_string();
            out.push(AstGrepMatch {
                file_path: rel_path.to_string(),
                node_kind: node.kind().to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_byte: start_b,
                end_byte: end_b,
                matched_text,
                enclosing_symbol: find_enclosing_symbol_label(node, source),
                metavars: env.to_captured_map(source),
            });
            // Skip descending into children of an already-matched node to avoid duplicate inner matches
            // when a single-metavar root pattern is used.
            if matches!(compiled.root, PatternNode::MetaVar(_)) {
                return;
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if out.len() >= max_matches {
            break;
        }
        collect_matches_dfs(
            child,
            rel_path,
            source,
            compiled,
            filter,
            inside_compiled,
            not_inside_compiled,
            has_compiled,
            max_matches,
            out,
        );
    }
}

fn is_supported_ast_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "py" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts"
    )
}

fn resolve_candidate_files(
    workspace_root: &Path,
    pattern_str: &str,
    file_pattern: Option<&str>,
) -> Vec<(PathBuf, String, String)> {
    // Extract a fixed literal from the pattern (outside $METAVARS) for trigram candidate pre-filtering
    let sample_fixed = CompiledPattern::compile(pattern_str, "rs")
        .or_else(|_| CompiledPattern::compile(pattern_str, "ts"))
        .or_else(|_| CompiledPattern::compile(pattern_str, "py"))
        .ok()
        .map(|c| c.root.fixed_string().to_string())
        .unwrap_or_default();

    // Check if `file_pattern` is a direct relative file path
    if let Some(fp) = file_pattern {
        let direct = workspace_root.join(fp);
        if direct.is_file() {
            let ext = direct
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            if is_supported_ast_extension(&ext) {
                return vec![(direct, fp.to_string(), ext)];
            }
        }
    }

    let mut out = Vec::new();
    if let Ok(trigram_idx) =
        crate::context::search::trigram::TrigramIndex::get_or_build_cached(workspace_root)
    {
        let plan = if sample_fixed.len() >= 3 {
            crate::context::search::trigram::build_query_plan(&sample_fixed, false)
        } else {
            crate::context::search::trigram::QueryPlan::MatchAll
        };
        for rel_path in trigram_idx.query_candidate_files(&plan, file_pattern) {
            let full = workspace_root.join(rel_path);
            let ext = full
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            if is_supported_ast_extension(&ext) {
                out.push((full, rel_path.to_string(), ext));
            }
        }
        return out;
    }

    for full in crate::context::walker::WorkspaceWalker::new(workspace_root)
        .extensions(&["rs", "py", "ts", "tsx", "js", "jsx"])
        .collect_absolute_files()
    {
        let rel_path = full
            .strip_prefix(workspace_root)
            .unwrap_or(&full)
            .to_string_lossy()
            .to_string();
        if let Some(pat) = file_pattern {
            let fname = full
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&rel_path);
            if !crate::utils::strings::wildcard_match(&rel_path, pat)
                && !crate::utils::strings::wildcard_match(fname, pat)
            {
                continue;
            }
        }
        let ext = full
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        out.push((full, rel_path, ext));
    }
    out
}

/// Executes a workspace-wide or file-scoped structural AST pattern search (`ast_grep_search`).
pub fn ast_grep_search(
    workspace_root: &Path,
    pattern_str: &str,
    filter: &AstGrepFilter,
    limit: usize,
) -> Result<Vec<AstGrepMatch>> {
    let max_results = limit.clamp(1, MAX_SEARCH_RESULTS);
    let candidates =
        resolve_candidate_files(workspace_root, pattern_str, filter.file_pattern.as_deref());

    let mut all_matches = Vec::new();
    for (full_path, rel_path, ext) in candidates {
        if all_matches.len() >= max_matches_cap(max_results) {
            break;
        }
        let Ok(content) = fs::read_to_string(&full_path) else {
            continue;
        };
        let remaining = max_matches_cap(max_results).saturating_sub(all_matches.len());
        if let Ok(file_matches) =
            find_matches_in_source(&rel_path, &ext, &content, pattern_str, filter, remaining)
        {
            all_matches.extend(file_matches);
        }
    }
    all_matches.truncate(max_results);
    Ok(all_matches)
}

fn max_matches_cap(limit: usize) -> usize {
    limit.min(MAX_SEARCH_RESULTS)
}

/// Formats [`AstGrepMatch`] results into clean Markdown with captured `$METAVAR` bindings and enclosing AST scope.
pub fn format_ast_grep_matches(pattern: &str, matches: &[AstGrepMatch]) -> String {
    if matches.is_empty() {
        return format!(
            "ℹ No structural AST matches found for pattern `{}`.",
            pattern
        );
    }

    let mut out = format!(
        "🌳 **AST Structural Pattern Search** for `{}` ({} matches):\n\n",
        pattern,
        matches.len()
    );

    for (idx, m) in matches.iter().enumerate() {
        let scope_badge = m
            .enclosing_symbol
            .as_deref()
            .map(|s| format!(" • `[in {}]`", s))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}. **`{}:{}-{}`** (`{}`){}\n",
            idx + 1,
            m.file_path,
            m.start_line,
            m.end_line,
            m.node_kind,
            scope_badge
        ));
        if !m.metavars.is_empty() {
            let mut vars: Vec<(&String, &CapturedVar)> = m.metavars.iter().collect();
            vars.sort_by_key(|(k, _)| *k);
            let formatted_vars: Vec<String> = vars
                .into_iter()
                .map(|(k, v)| {
                    let compact = v.text.split_whitespace().collect::<Vec<_>>().join(" ");
                    format!(
                        "`${}` = `{}`",
                        k,
                        crate::utils::strings::truncate_display(&compact, 60)
                    )
                })
                .collect();
            out.push_str(&format!(
                "   - **Bindings:** {}\n",
                formatted_vars.join(", ")
            ));
        }
        out.push_str(&format!("   ```\n   {}\n   ```\n\n", m.matched_text.trim()));
    }

    out
}

/// Computes the leading indentation (in spaces) of the line containing `byte_offset`.
fn get_line_indent_at_byte(source: &str, byte_offset: usize) -> usize {
    let prefix = source.get(..byte_offset).unwrap_or("");
    let line_start = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    source
        .get(line_start..byte_offset)
        .unwrap_or("")
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// De-indents lines `1..` of a multi-line slice by `source_indent` and re-indents by `target_indent`
/// (`ast-grep` `extract_with_deindent` + `indent_lines`).
fn reindent_multiline_slice(text: &str, source_indent: usize, target_indent: usize) -> String {
    if !text.contains('\n') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 32);
    for (idx, line) in text.lines().enumerate() {
        if idx == 0 {
            out.push_str(line);
            continue;
        }
        out.push('\n');
        if line.trim().is_empty() {
            continue;
        }
        let line_spaces = line.chars().take_while(|c| *c == ' ').count();
        let relative = line_spaces.saturating_sub(source_indent);
        let new_indent = target_indent + relative;
        for _ in 0..new_indent {
            out.push(' ');
        }
        out.push_str(line.trim_start_matches(' '));
    }
    out
}

/// Renders a replacement template by substituting `$VAR` and `$$$VAR` bindings with relative indentation preservation
/// (`ast-grep` `TemplateFix::generate_replacement`).
pub fn render_replacement_template(
    template: &str,
    match_hit: &AstGrepMatch,
    source: &str,
) -> String {
    let mut rendered = String::with_capacity(template.len() + 32);
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            let var_offset_in_tmpl = rendered.len();
            let tmpl_indent = get_line_indent_at_byte(&rendered, var_offset_in_tmpl);

            // Check $$$NAME
            if i + 2 < chars.len() && chars[i + 1] == '$' && chars[i + 2] == '$' {
                let mut j = i + 3;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let name: String = chars[i + 3..j].iter().collect();
                if let Some(cap) = match_hit.metavars.get(&name) {
                    let src_indent = get_line_indent_at_byte(source, cap.start_byte);
                    rendered.push_str(&reindent_multiline_slice(
                        &cap.text,
                        src_indent,
                        tmpl_indent,
                    ));
                    i = j;
                    continue;
                }
            }

            // Check $NAME
            if i + 1 < chars.len() && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_') {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                if let Some(cap) = match_hit.metavars.get(&name) {
                    let src_indent = get_line_indent_at_byte(source, cap.start_byte);
                    rendered.push_str(&reindent_multiline_slice(
                        &cap.text,
                        src_indent,
                        tmpl_indent,
                    ));
                    i = j;
                    continue;
                }
            }
        }
        rendered.push(chars[i]);
        i += 1;
    }

    let node_indent = get_line_indent_at_byte(source, match_hit.start_byte);
    reindent_multiline_slice(&rendered, 0, node_indent)
}

/// Executes a structural AST search-and-replace across matching files (`ast_grep_rewrite`),
/// validating every modified file with [`SyntaxGuard`] before writing to disk.
pub fn ast_grep_rewrite(
    workspace_root: &Path,
    pattern_str: &str,
    replacement_template: &str,
    filter: &AstGrepFilter,
    dry_run: bool,
) -> Result<AstGrepRewriteReport> {
    let candidates =
        resolve_candidate_files(workspace_root, pattern_str, filter.file_pattern.as_deref());

    let mut total_matches = 0usize;
    let mut files_modified = Vec::new();
    let mut diff_preview = String::new();

    for (full_path, rel_path, ext) in candidates {
        let Ok(original_content) = fs::read_to_string(&full_path) else {
            continue;
        };
        let Ok(raw_matches) = find_matches_in_source(
            &rel_path,
            &ext,
            &original_content,
            pattern_str,
            filter,
            MAX_SEARCH_RESULTS,
        ) else {
            continue;
        };
        if raw_matches.is_empty() {
            continue;
        }

        // Keep outermost non-overlapping matches sorted by byte offset
        let mut non_overlapping: Vec<AstGrepMatch> = Vec::new();
        for m in raw_matches {
            if non_overlapping
                .last()
                .is_none_or(|prev| m.start_byte >= prev.end_byte)
            {
                non_overlapping.push(m);
            }
        }

        let mut new_content = original_content.clone();
        let mut file_diff = format!("--- a/{}\n+++ b/{}\n", rel_path, rel_path);

        // Apply replacements in reverse byte order so earlier byte offsets remain valid
        for m in non_overlapping.iter().rev() {
            let rendered = render_replacement_template(replacement_template, m, &original_content);
            if m.start_byte <= m.end_byte
                && m.end_byte <= new_content.len()
                && new_content.is_char_boundary(m.start_byte)
                && new_content.is_char_boundary(m.end_byte)
            {
                new_content.replace_range(m.start_byte..m.end_byte, &rendered);
            }
        }

        for m in &non_overlapping {
            let rendered = render_replacement_template(replacement_template, m, &original_content);
            file_diff.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                m.start_line,
                m.end_line.saturating_sub(m.start_line) + 1,
                m.start_line,
                rendered.lines().count().max(1)
            ));
            for l in m.matched_text.lines() {
                file_diff.push_str(&format!("-{}\n", l));
            }
            for l in rendered.lines() {
                file_diff.push_str(&format!("+{}\n", l));
            }
        }

        if new_content == original_content {
            continue;
        }

        // Pre-disk Tree-sitter SyntaxGuard validation
        if let Err(diag) =
            SyntaxGuard::check_syntax_barrier(&full_path, &original_content, &new_content)
        {
            return Err(ToolError::InvalidArguments {
                name: "ast_grep_rewrite".to_string(),
                reason: diag,
            }
            .into());
        }

        if !dry_run {
            fs::write(&full_path, &new_content).map_err(|e| ToolError::FileOp {
                path: rel_path.clone(),
                source: e,
            })?;
        }

        total_matches += non_overlapping.len();
        files_modified.push(rel_path);
        diff_preview.push_str(&file_diff);
    }

    if !dry_run && !files_modified.is_empty() {
        crate::context::graph::CodeGraph::invalidate_cache();
        crate::context::index::SymbolIndex::invalidate_cache();
        crate::context::search::trigram::TrigramIndex::invalidate_cache();
    }

    Ok(AstGrepRewriteReport {
        pattern: pattern_str.to_string(),
        replacement: replacement_template.to_string(),
        dry_run,
        total_matches,
        files_modified,
        diff_preview,
    })
}

/// Collects exact byte ranges of AST identifier nodes matching `target_symbol`
/// (skipping comments and string literals) for safe AST-aware symbol renaming.
pub fn find_identifier_occurrences_ast(
    ext: &str,
    source: &str,
    target_symbol: &str,
) -> Option<Vec<(usize, usize)>> {
    let lang = SyntaxGuard::language_for_extension(ext)?;
    let mut parser = Parser::new();
    parser.set_language(&lang).ok()?;
    let tree = parser.parse(source, None)?;

    let mut spans = Vec::new();
    collect_ident_spans_dfs(tree.root_node(), source, target_symbol, &mut spans);
    Some(spans)
}

fn collect_ident_spans_dfs(
    node: Node<'_>,
    source: &str,
    target_symbol: &str,
    out: &mut Vec<(usize, usize)>,
) {
    if is_comment_node(node) {
        return;
    }
    let kind = node.kind();
    if kind.contains("string") || kind.contains("char_literal") || kind.contains("template_string")
    {
        return;
    }

    if node.child_count() == 0 && (kind.contains("identifier") || kind == "self" || kind == "crate")
    {
        let text = source.get(node.start_byte()..node.end_byte()).unwrap_or("");
        if text == target_symbol {
            out.push((node.start_byte(), node.end_byte()));
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ident_spans_dfs(child, source, target_symbol, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ast_grep_metavar_equality_and_variadic_matching() {
        let code = r#"
pub fn check_values(a: i32, b: i32) -> bool {
    // a == a inside a comment should be ignored
    if a == b {
        println!("diff: {} {}", a, b);
    }
    a == a
}
"#;
        // 1. Repeated metavariable `$X == $X` must ONLY match `a == a`, not `a == b` or the comment
        let eq_matches = find_matches_in_source(
            "check.rs",
            "rs",
            code,
            "$X == $X",
            &AstGrepFilter::default(),
            10,
        )
        .unwrap();
        assert_eq!(eq_matches.len(), 1);
        assert_eq!(eq_matches[0].matched_text, "a == a");
        assert_eq!(eq_matches[0].metavars.get("X").unwrap().text, "a");

        // 2. Variadic `println!($FMT, $$$ARGS)` captures format string and remaining arguments
        let macro_matches = find_matches_in_source(
            "check.rs",
            "rs",
            code,
            "println!($FMT, $$$ARGS)",
            &AstGrepFilter::default(),
            10,
        )
        .unwrap();
        assert_eq!(macro_matches.len(), 1);
        assert_eq!(
            macro_matches[0].metavars.get("FMT").unwrap().text,
            "\"diff: {} {}\""
        );
        assert_eq!(macro_matches[0].metavars.get("ARGS").unwrap().text, "a, b");
    }

    #[test]
    fn test_ast_grep_relational_filters_and_rewrite() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let rs_file = ws.join("handler.rs");
        fs::write(
            &rs_file,
            r#"pub fn prod_handler(input: Option<i32>) -> Option<i32> {
    let val = input.unwrap();
    Some(val + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_prod() {
        let x = prod_handler(Some(1)).unwrap();
        assert_eq!(x, 2);
    }
}
"#,
        )
        .unwrap();

        // Rewrite `$EXPR.unwrap()` -> `$EXPR?` ONLY when `not_inside: "mod tests"`
        let filter = AstGrepFilter {
            inside: Some("function_item".to_string()),
            not_inside: Some("mod tests".to_string()),
            has: None,
            file_pattern: Some("*.rs".to_string()),
        };

        let report = ast_grep_rewrite(ws, "$EXPR.unwrap()", "$EXPR?", &filter, false).unwrap();
        assert_eq!(report.total_matches, 1);
        assert_eq!(report.files_modified, vec!["handler.rs".to_string()]);

        let updated = fs::read_to_string(&rs_file).unwrap();
        assert!(updated.contains("let val = input?;"));
        // The `.unwrap()` inside `mod tests` must remain untouched!
        assert!(updated.contains("let x = prod_handler(Some(1)).unwrap();"));
    }

    #[test]
    fn test_find_identifier_occurrences_ast_ignores_strings_and_comments() {
        let code = r#"
// old_token in comment should not be renamed
pub fn old_token() -> &'static str {
    "old_token in string literal should not be renamed"
}
"#;
        let spans = find_identifier_occurrences_ast("rs", code, "old_token").unwrap();
        assert_eq!(spans.len(), 1);
        let (s, e) = spans[0];
        assert_eq!(&code[s..e], "old_token");
    }
}
