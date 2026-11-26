use crate::context::ast_transform::AstNodeInfo;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstSymbolDelta {
    pub kind: String,
    pub name: String,
    pub is_public: bool,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstModifiedDelta {
    pub kind: String,
    pub name: String,
    pub is_public: bool,
    pub old_start_line: usize,
    pub old_end_line: usize,
    pub new_start_line: usize,
    pub new_end_line: usize,
    pub signature_changed: bool,
    pub body_changed: bool,
    pub old_signature: String,
    pub new_signature: String,
    /// Concise structural AST atom changes inside the symbol computed by bounded Dijkstra diff.
    #[serde(default)]
    pub structural_changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BreakingChange {
    pub symbol_name: String,
    pub kind: String,
    pub reason: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstDeltaReport {
    pub file_path: String,
    pub added: Vec<AstSymbolDelta>,
    pub removed: Vec<AstSymbolDelta>,
    pub modified: Vec<AstModifiedDelta>,
    pub breaking_changes: Vec<BreakingChange>,
    pub unchanged_count: usize,
    /// Symbols whose raw text changed only in whitespace/formatting or comments (0 semantic AST delta).
    #[serde(default)]
    pub formatting_only_count: usize,
}

impl AstDeltaReport {
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    pub fn format_markdown(&self) -> String {
        let mut out = format!("# Semantic AST Diff: `{}`\n\n", self.file_path);

        if !self.breaking_changes.is_empty() {
            out.push_str("### ⚠️ Potential Breaking Changes\n");
            for b in &self.breaking_changes {
                out.push_str(&format!(
                    "- **[{}]** `{}` ({}) — {}\n",
                    b.severity, b.symbol_name, b.kind, b.reason
                ));
            }
            out.push('\n');
        }

        if !self.added.is_empty() {
            out.push_str("### ➕ Added Symbols\n");
            for a in &self.added {
                let vis = if a.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** (lines {}-{})\n  ```\n  {}\n  ```\n",
                    a.kind, vis, a.name, a.start_line, a.end_line, a.signature
                ));
            }
            out.push('\n');
        }

        if !self.removed.is_empty() {
            out.push_str("### ➖ Removed Symbols\n");
            for r in &self.removed {
                let vis = if r.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** (was lines {}-{})\n  ```\n  {}\n  ```\n",
                    r.kind, vis, r.name, r.start_line, r.end_line, r.signature
                ));
            }
            out.push('\n');
        }

        if !self.modified.is_empty() {
            out.push_str("### 🔄 Modified Symbols\n");
            for m in &self.modified {
                let change_type = if m.signature_changed {
                    "Signature & Body Modified"
                } else {
                    "Body Modified (Signature Stable)"
                };
                let vis = if m.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** — *{}* (lines {}->{})\n",
                    m.kind, vis, m.name, change_type, m.old_start_line, m.new_start_line
                ));
                if m.signature_changed {
                    out.push_str(&format!(
                        "  - **Old:** `{}`\n  - **New:** `{}`\n",
                        m.old_signature, m.new_signature
                    ));
                }
                for change in &m.structural_changes {
                    out.push_str(&format!("  - **AST Delta:** {}\n", change));
                }
            }
            out.push('\n');
        }

        let fmt_note = if self.formatting_only_count > 0 {
            format!(", {} formatting/comment-only", self.formatting_only_count)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "📊 **Summary:** {} added, {} removed, {} modified, {} unchanged{}.\n",
            self.added.len(),
            self.removed.len(),
            self.modified.len(),
            self.unchanged_count,
            fmt_note
        ));

        out
    }
}

pub struct AstDiffEngine;

impl AstDiffEngine {
    /// Extracts symbols from in-memory source content based on language extension
    pub fn parse_source(ext: &str, content: &str) -> Result<Vec<AstNodeInfo>> {
        let lang = crate::context::syntax_guard::SyntaxGuard::language_for_extension(ext)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "ast_diff".to_string(),
                reason: format!("Unsupported file extension for AST diff: '{}'", ext),
            })?;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&lang)
            .map_err(|e| ToolError::CommandExec(format!("Tree-sitter parser error: {}", e)))?;

        let mut nodes = Vec::new();
        if let Some(tree) = parser.parse(content, None) {
            match ext {
                "rs" => crate::context::ast_transform::AstTransformer::traverse_rust(
                    tree.root_node(),
                    content,
                    &mut nodes,
                ),
                "py" => crate::context::ast_transform::AstTransformer::traverse_python(
                    tree.root_node(),
                    content,
                    &mut nodes,
                ),
                _ => crate::context::ast_transform::AstTransformer::traverse_ts(
                    tree.root_node(),
                    content,
                    &mut nodes,
                ),
            }
        }
        Ok(nodes)
    }

    /// Computes structural AST delta between old source code and new source code
    pub fn diff_sources(
        file_path: &str,
        ext: &str,
        old_source: &str,
        new_source: &str,
    ) -> Result<AstDeltaReport> {
        let old_nodes = Self::parse_source(ext, old_source)?;
        let new_nodes = Self::parse_source(ext, new_source)?;

        let mut old_map: HashMap<(String, String), AstNodeInfo> = HashMap::new();
        for node in old_nodes {
            old_map.insert((node.kind.clone(), node.name.clone()), node);
        }

        let mut new_map: HashMap<(String, String), AstNodeInfo> = HashMap::new();
        for node in new_nodes {
            new_map.insert((node.kind.clone(), node.name.clone()), node);
        }

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();
        let mut breaking_changes = Vec::new();
        let mut unchanged_count = 0;
        let mut formatting_only_count = 0;

        // Find added & modified
        for (key, new_node) in &new_map {
            if let Some(old_node) = old_map.get(key) {
                if old_node.snippet.trim() == new_node.snippet.trim() {
                    unchanged_count += 1;
                } else {
                    // Difftastic-inspired structural AST atom comparison: ignore pure whitespace/
                    // formatting or comment-only diffs instead of falsely flagging `body_changed`.
                    let old_atoms =
                        Self::extract_syntax_atoms(ext, &old_node.snippet, old_node.start_line);
                    let new_atoms =
                        Self::extract_syntax_atoms(ext, &new_node.snippet, new_node.start_line);

                    let structurally_identical = !old_atoms.is_empty()
                        && old_atoms.len() == new_atoms.len()
                        && old_atoms
                            .iter()
                            .zip(new_atoms.iter())
                            .all(|(a, b)| a.kind == b.kind && a.text == b.text);

                    if structurally_identical {
                        unchanged_count += 1;
                        formatting_only_count += 1;
                        continue;
                    }

                    let old_sig = Self::extract_signature(&old_node.snippet);
                    let new_sig = Self::extract_signature(&new_node.snippet);
                    let sig_changed = old_sig != new_sig;

                    if new_node.is_public && sig_changed {
                        breaking_changes.push(BreakingChange {
                            symbol_name: new_node.name.clone(),
                            kind: new_node.kind.clone(),
                            reason: format!(
                                "Public signature changed from `{}` to `{}`",
                                old_sig, new_sig
                            ),
                            severity: "HIGH".to_string(),
                        });
                    }

                    let structural_changes =
                        Self::dijkstra_ast_atom_diff(&old_atoms, &new_atoms, 6);

                    modified.push(AstModifiedDelta {
                        kind: new_node.kind.clone(),
                        name: new_node.name.clone(),
                        is_public: new_node.is_public,
                        old_start_line: old_node.start_line,
                        old_end_line: old_node.end_line,
                        new_start_line: new_node.start_line,
                        new_end_line: new_node.end_line,
                        signature_changed: sig_changed,
                        body_changed: true,
                        old_signature: old_sig,
                        new_signature: new_sig,
                        structural_changes,
                    });
                }
            } else {
                added.push(AstSymbolDelta {
                    kind: new_node.kind.clone(),
                    name: new_node.name.clone(),
                    is_public: new_node.is_public,
                    start_line: new_node.start_line,
                    end_line: new_node.end_line,
                    signature: Self::extract_signature(&new_node.snippet),
                });
            }
        }

        // Find removed
        for (key, old_node) in &old_map {
            if !new_map.contains_key(key) {
                if old_node.is_public {
                    breaking_changes.push(BreakingChange {
                        symbol_name: old_node.name.clone(),
                        kind: old_node.kind.clone(),
                        reason: "Public symbol was removed from definition scope".to_string(),
                        severity: "HIGH".to_string(),
                    });
                }

                removed.push(AstSymbolDelta {
                    kind: old_node.kind.clone(),
                    name: old_node.name.clone(),
                    is_public: old_node.is_public,
                    start_line: old_node.start_line,
                    end_line: old_node.end_line,
                    signature: Self::extract_signature(&old_node.snippet),
                });
            }
        }

        // Sort for deterministic reporting
        added.sort_by_key(|c| c.start_line);
        removed.sort_by_key(|c| c.start_line);
        modified.sort_by_key(|c| c.new_start_line);

        Ok(AstDeltaReport {
            file_path: file_path.to_string(),
            added,
            removed,
            modified,
            breaking_changes,
            unchanged_count,
            formatting_only_count,
        })
    }

    /// Diffs a workspace file against its git HEAD baseline or provided new content
    pub fn diff_file(
        workspace_root: &Path,
        file_path: &str,
        new_content_override: Option<&str>,
    ) -> Result<AstDeltaReport> {
        let full_path = workspace_root.join(file_path);
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let new_content = if let Some(override_text) = new_content_override {
            override_text.to_string()
        } else if full_path.exists() {
            std::fs::read_to_string(&full_path).map_err(|e| ToolError::FileOp {
                path: file_path.to_string(),
                source: e,
            })?
        } else {
            String::new()
        };

        // Try reading git HEAD version as baseline; if untracked or git fails, use empty string
        let old_content = match std::process::Command::new("git")
            .current_dir(workspace_root)
            .args(["show", &format!("HEAD:{}", file_path)])
            .output()
        {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            _ => String::new(),
        };

        Self::diff_sources(file_path, ext, &old_content, &new_content)
    }

    fn extract_signature(snippet: &str) -> String {
        let first_line = snippet.lines().next().unwrap_or("").trim();
        if let Some(pos) = first_line.find('{') {
            first_line[..pos].trim().to_string()
        } else {
            first_line.to_string()
        }
    }

    /// Flattens a Tree-sitter parsed symbol snippet into typed leaf syntax atoms (skipping
    /// comments and pure whitespace) for formatting-invariant structural diffing (`difftastic` model).
    fn extract_syntax_atoms(ext: &str, snippet: &str, base_line: usize) -> Vec<SyntaxAtom> {
        let Some(lang) = crate::context::syntax_guard::SyntaxGuard::language_for_extension(ext)
        else {
            return Vec::new();
        };
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&lang).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(snippet, None) else {
            return Vec::new();
        };
        let mut atoms = Vec::new();
        collect_syntax_atoms_dfs(tree.root_node(), snippet, base_line, &mut atoms);
        // Strip pure formatting trailing commas immediately before closing delimiters (`)`, `]`, `}`)
        // so `rustfmt`/`prettier` single-line <-> multi-line reflows are invariant.
        let mut normalized = Vec::with_capacity(atoms.len());
        for i in 0..atoms.len() {
            let is_trailing_comma = atoms[i].text == ","
                && atoms
                    .get(i + 1)
                    .is_some_and(|next| matches!(next.text.as_str(), ")" | "]" | "}"));
            if !is_trailing_comma {
                normalized.push(atoms[i].clone());
            }
        }
        normalized
    }

    /// Bounded Dijkstra shortest-path edit graph over Tree-sitter syntax atoms (`Wilfred/difftastic`
    /// algorithm). Uses weighted AST transitions (exact match=0, same-kind substitution=2,
    /// delimiter insert/delete=2, semantic atom insert/delete=3) with an explicit vertex cap so
    /// even very large symbols finish deterministically in milliseconds.
    fn dijkstra_ast_atom_diff(
        old_atoms: &[SyntaxAtom],
        new_atoms: &[SyntaxAtom],
        max_hunks: usize,
    ) -> Vec<String> {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;

        let l = old_atoms.len();
        let r = new_atoms.len();
        if l == 0 && r == 0 {
            return Vec::new();
        }

        // Bounded vertex budget: fall back to token-level Myers diff (`similar`) on huge symbols
        const MAX_STATES: usize = 25_000;
        if l.saturating_mul(r) > 400_000 {
            return Self::fallback_atom_diff(old_atoms, new_atoms, max_hunks);
        }

        let mut dist: HashMap<(usize, usize), u32> = HashMap::new();
        let mut parent: HashMap<(usize, usize), (usize, usize, AstEditOp)> = HashMap::new();
        let mut heap: BinaryHeap<Reverse<(u32, usize, usize)>> = BinaryHeap::new();

        dist.insert((0, 0), 0);
        heap.push(Reverse((0, 0, 0)));

        let mut popped = 0usize;
        let mut reached_goal = false;

        while let Some(Reverse((cost, i, j))) = heap.pop() {
            if i == l && j == r {
                reached_goal = true;
                break;
            }
            if dist.get(&(i, j)).copied().unwrap_or(u32::MAX) < cost {
                continue;
            }
            popped += 1;
            if popped > MAX_STATES {
                break;
            }

            // 1. Exact structural match (cost 0)
            if i < l
                && j < r
                && old_atoms[i].kind == new_atoms[j].kind
                && old_atoms[i].text == new_atoms[j].text
            {
                let next = (i + 1, j + 1);
                if cost < dist.get(&next).copied().unwrap_or(u32::MAX) {
                    dist.insert(next, cost);
                    parent.insert(next, (i, j, AstEditOp::Match));
                    heap.push(Reverse((cost, next.0, next.1)));
                }
                continue;
            }

            // 2. Same-kind semantic leaf substitution (e.g., renamed identifier or updated literal, cost 2)
            if i < l
                && j < r
                && old_atoms[i].kind == new_atoms[j].kind
                && !is_delimiter_atom(&old_atoms[i].text)
            {
                let next = (i + 1, j + 1);
                let nc = cost + 2;
                if nc < dist.get(&next).copied().unwrap_or(u32::MAX) {
                    dist.insert(next, nc);
                    parent.insert(next, (i, j, AstEditOp::Replace));
                    heap.push(Reverse((nc, next.0, next.1)));
                }
            }

            // 3. Delete old atom (delimiter=2, semantic=3)
            if i < l {
                let step = if is_delimiter_atom(&old_atoms[i].text) {
                    2
                } else {
                    3
                };
                let next = (i + 1, j);
                let nc = cost + step;
                if nc < dist.get(&next).copied().unwrap_or(u32::MAX) {
                    dist.insert(next, nc);
                    parent.insert(next, (i, j, AstEditOp::Delete));
                    heap.push(Reverse((nc, next.0, next.1)));
                }
            }

            // 4. Insert new atom (delimiter=2, semantic=3)
            if j < r {
                let step = if is_delimiter_atom(&new_atoms[j].text) {
                    2
                } else {
                    3
                };
                let next = (i, j + 1);
                let nc = cost + step;
                if nc < dist.get(&next).copied().unwrap_or(u32::MAX) {
                    dist.insert(next, nc);
                    parent.insert(next, (i, j, AstEditOp::Insert));
                    heap.push(Reverse((nc, next.0, next.1)));
                }
            }
        }

        if !reached_goal {
            return Self::fallback_atom_diff(old_atoms, new_atoms, max_hunks);
        }

        // Backtrack optimal edit script
        let mut ops: Vec<(AstEditOp, usize, usize)> = Vec::new();
        let mut cur = (l, r);
        while cur != (0, 0) {
            let Some(&(pi, pj, op)) = parent.get(&cur) else {
                break;
            };
            ops.push((op, pi, pj));
            cur = (pi, pj);
        }
        ops.reverse();

        // Group consecutive non-Match edits into compact human/LLM-readable structural hunks
        let mut hunks = Vec::new();
        let mut cur_removed: Vec<&str> = Vec::new();
        let mut cur_added: Vec<&str> = Vec::new();
        let mut hunk_line: Option<usize> = None;

        let flush_hunk = |removed: &mut Vec<&str>,
                          added: &mut Vec<&str>,
                          line_opt: &mut Option<usize>,
                          out: &mut Vec<String>| {
            if removed.is_empty() && added.is_empty() {
                return;
            }
            let ln = line_opt.unwrap_or(1);
            let rem_s = removed.join(" ");
            let add_s = added.join(" ");
            let desc = match (removed.is_empty(), added.is_empty()) {
                (false, false) => format!("L{ln}: `{rem_s}` -> `{add_s}`"),
                (false, true) => format!("L{ln}: removed `{rem_s}`"),
                (true, false) => format!("L{ln}: added `{add_s}`"),
                (true, true) => String::new(),
            };
            if !desc.is_empty() && out.len() < max_hunks {
                out.push(desc);
            }
            removed.clear();
            added.clear();
            *line_opt = None;
        };

        for (op, i, j) in ops {
            match op {
                AstEditOp::Match => {
                    flush_hunk(&mut cur_removed, &mut cur_added, &mut hunk_line, &mut hunks)
                }
                AstEditOp::Replace => {
                    hunk_line.get_or_insert(new_atoms[j].line);
                    cur_removed.push(old_atoms[i].text.as_str());
                    cur_added.push(new_atoms[j].text.as_str());
                }
                AstEditOp::Delete => {
                    hunk_line.get_or_insert(old_atoms[i].line);
                    cur_removed.push(old_atoms[i].text.as_str());
                }
                AstEditOp::Insert => {
                    hunk_line.get_or_insert(new_atoms[j].line);
                    cur_added.push(new_atoms[j].text.as_str());
                }
            }
            if hunks.len() >= max_hunks {
                break;
            }
        }
        flush_hunk(&mut cur_removed, &mut cur_added, &mut hunk_line, &mut hunks);
        hunks
    }

    fn fallback_atom_diff(
        old_atoms: &[SyntaxAtom],
        new_atoms: &[SyntaxAtom],
        max_hunks: usize,
    ) -> Vec<String> {
        let old_tokens: Vec<&str> = old_atoms.iter().map(|a| a.text.as_str()).collect();
        let new_tokens: Vec<&str> = new_atoms.iter().map(|a| a.text.as_str()).collect();
        let diff = similar::TextDiff::from_slices(&old_tokens, &new_tokens);
        let mut hunks = Vec::new();
        for op in diff.ops() {
            if matches!(op.tag(), similar::DiffTag::Equal) {
                continue;
            }
            let rem = old_tokens[op.old_range()].join(" ");
            let add = new_tokens[op.new_range()].join(" ");
            let ln = new_atoms
                .get(op.new_range().start)
                .or_else(|| old_atoms.get(op.old_range().start))
                .map(|a| a.line)
                .unwrap_or(1);
            let desc = match (rem.is_empty(), add.is_empty()) {
                (false, false) => format!("L{ln}: `{rem}` -> `{add}`"),
                (false, true) => format!("L{ln}: removed `{rem}`"),
                (true, false) => format!("L{ln}: added `{add}`"),
                (true, true) => continue,
            };
            hunks.push(desc);
            if hunks.len() >= max_hunks {
                break;
            }
        }
        hunks
    }
}

#[derive(Debug, Clone)]
struct SyntaxAtom {
    kind: String,
    text: String,
    line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AstEditOp {
    Match,
    Replace,
    Delete,
    Insert,
}

fn is_delimiter_atom(text: &str) -> bool {
    matches!(text, "(" | ")" | "{" | "}" | "[" | "]" | "," | ";" | ":")
}

fn collect_syntax_atoms_dfs(
    node: tree_sitter::Node<'_>,
    source: &str,
    base_line: usize,
    out: &mut Vec<SyntaxAtom>,
) {
    let kind = node.kind();
    if node.is_extra() || kind.contains("comment") {
        return;
    }
    if node.child_count() == 0 {
        if let Some(raw) = source.get(node.start_byte()..node.end_byte()) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                out.push(SyntaxAtom {
                    kind: kind.to_string(),
                    text: trimmed.to_string(),
                    line: base_line + node.start_position().row,
                });
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_syntax_atoms_dfs(child, source, base_line, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_diff_rust_add_remove_modify() {
        let old_rs = r#"
pub fn calculate_total(a: i32, b: i32) -> i32 {
    a + b
}

fn helper() {
    println!("old");
}
"#;

        let new_rs = r#"
pub fn calculate_total(a: i32, b: i32, factor: f32) -> f32 {
    (a + b) as f32 * factor
}

pub struct Invoice {
    pub id: u64,
}
"#;

        let report = AstDiffEngine::diff_sources("src/lib.rs", "rs", old_rs, new_rs).unwrap();

        // 1. Check added
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.added[0].name, "Invoice");
        assert_eq!(report.added[0].kind, "struct_item");

        // 2. Check removed
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].name, "helper");

        // 3. Check modified
        assert_eq!(report.modified.len(), 1);
        assert_eq!(report.modified[0].name, "calculate_total");
        assert!(report.modified[0].signature_changed);

        // 4. Check breaking change detection
        assert_eq!(report.breaking_changes.len(), 1);
        assert_eq!(report.breaking_changes[0].symbol_name, "calculate_total");
        assert_eq!(report.breaking_changes[0].severity, "HIGH");

        // 5. Markdown formatting
        let md = report.format_markdown();
        assert!(md.contains("Semantic AST Diff"));
        assert!(md.contains("Breaking Changes"));
    }

    #[test]
    fn test_ast_diff_python_classes_and_functions() {
        let old_py = r#"
class AuthManager:
    def login(self, user):
        pass

def fetch_data():
    return 42
"#;

        let new_py = r#"
class AuthManager:
    def login(self, user, token):
        pass

def fetch_data():
    return 42
"#;

        let report = AstDiffEngine::diff_sources("auth.py", "py", old_py, new_py).unwrap();
        assert_eq!(report.unchanged_count, 1); // fetch_data is unchanged
        assert_eq!(report.modified.len(), 2);
        assert!(report.modified.iter().any(|m| m.name == "AuthManager"));
        assert!(report.modified.iter().any(|m| m.name == "login"));
    }

    #[test]
    fn test_dijkstra_ast_diff_ignores_formatting_and_emits_structural_hunks() {
        let old_rs = r#"
pub fn compute(a: i32, b: i32) -> i32 {
    a + b
}

pub fn untouched(x: i32) -> i32 {
    x * 2
}
"#;
        // `untouched` is reformatted + given a comment only (zero semantic delta);
        // `compute` replaces `+` with `.saturating_add(...)`.
        let new_rs = r#"
pub fn compute(a: i32, b: i32) -> i32 {
    a.saturating_add(b)
}

pub fn untouched(
    x: i32,
) -> i32 {
    // pure comment and whitespace reflow
    x * 2
}
"#;
        let report = AstDiffEngine::diff_sources("src/math.rs", "rs", old_rs, new_rs).unwrap();
        assert_eq!(
            report.formatting_only_count, 1,
            "Formatting/comment-only edit to `untouched` must not pollute semantic diff"
        );
        assert_eq!(report.modified.len(), 1);
        assert_eq!(report.modified[0].name, "compute");
        assert!(
            !report.modified[0].structural_changes.is_empty(),
            "Dijkstra AST diff should emit concise structural atom changes"
        );
        assert!(
            report.modified[0]
                .structural_changes
                .iter()
                .any(|c| c.contains("saturating_add")),
            "Expected structural delta mentioning `saturating_add`, got: {:?}",
            report.modified[0].structural_changes
        );
    }
}
