//! Native Rust implementation of the `agent0ai/dox` hierarchical work-contract framework:
//! - Root DOX Rail + Scoped Child DOX Contracts (`DoxRole::RootRail`, `DoxRole::ChildContract`)
//! - Canonical 6-Section `ChildDocShape` parser & renderer (`Purpose`, `Ownership`,
//!   `Local Contracts`, `Work Guidance`, `Verification`, `Child DOX Index`)
//! - `## Child DOX Index` scope routing & bottom-up recursive index maintenance
//! - Unindexed project sentinel detection (`"This project is not yet indexed..."`) and
//!   automated durable boundary discovery (`initialize_dox_tree`)
//! - Post-edit DOX drift auditing & 6-Step Closeout verification (`DoxCloseoutReport`)
//! - Durable `## User Preferences` recording across root rail and child contracts.

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Sentinel phrase from `agent0ai/dox` indicating the project's `Child DOX Index` has not been built yet.
pub const UNINDEXED_DOX_SENTINEL: &str = "this project is not yet indexed";

/// Role of a DOX contract file in the repository hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoxRole {
    /// Repository root DOX rail: project-wide instructions, global preferences, durable workflow rules,
    /// and the top-level `Child DOX Index`.
    RootRail,
    /// Scoped subtree DOX contract: owns domain-specific instructions, local contracts, verification,
    /// and its own `Child DOX Index`.
    ChildContract,
}

impl DoxRole {
    #[must_use]
    pub fn as_label(self, is_nearest: bool) -> &'static str {
        match (self, is_nearest) {
            (Self::RootRail, true) => {
                "DOX Root Rail (Nearest Contract — Repo-Wide Rules & Top-Level Index)"
            }
            (Self::RootRail, false) => "DOX Root Rail (Repo-Wide Rules & Top-Level Index)",
            (Self::ChildContract, true) => {
                "DOX Nearest Local Contract (Controls Local Work Details — Cannot Weaken Root DOX)"
            }
            (Self::ChildContract, false) => "DOX Parent Subtree Contract",
        }
    }
}

/// An entry in a parent DOX document's `## Child DOX Index` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoxChildIndexEntry {
    /// Relative path from workspace root to the child DOX file (e.g., `src/agent/.minicode/minicode.md`).
    pub child_doc_path: String,
    /// Normalized directory prefix covered by this child contract (e.g., `src/agent`).
    pub scope_prefix: String,
    /// Concise summary of what the child contract covers.
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoxSectionArchetype {
    Purpose,
    Ownership,
    LocalContracts,
    WorkGuidance,
    Verification,
    UserPreferences,
    ChildDoxIndex,
}

/// Structured representation of a DOX Markdown contract (`agent0ai/dox` Child Doc Shape + Root Rail).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoxDocument {
    /// Relative path to this DOX file from workspace root.
    pub rel_path: String,
    /// Relative directory scope owned by this DOX file (`""` for workspace root).
    pub scope_dir: String,
    /// Document role (`RootRail` vs `ChildContract`).
    pub role: DoxRole,
    /// Top-level H1 heading title.
    pub title: String,
    /// Optional preamble lines under H1 before the first H2 section.
    pub preamble: Vec<String>,
    /// `## Purpose` — why this boundary exists and what domain it governs.
    pub purpose: Vec<String>,
    /// `## Ownership` — files, modules, and assets directly owned by this scope.
    pub ownership: Vec<String>,
    /// `## Local Contracts` — invariants, interfaces, and architectural boundaries.
    pub local_contracts: Vec<String>,
    /// `## Work Guidance` — current coding/operational standards for this scope (empty if none yet).
    pub work_guidance: Vec<String>,
    /// `## Verification` — existing verification checks/commands for this scope (empty if none yet).
    pub verification: Vec<String>,
    /// `## User Preferences` — durable user preferences recorded at root or subtree level.
    pub user_preferences: Vec<String>,
    /// `## Child DOX Index` — direct child DOX contracts beneath this scope.
    pub child_index: Vec<DoxChildIndexEntry>,
    /// Files or subdirectories explicitly noted in `## Child DOX Index` as staying owned by the parent.
    pub parent_owned_items: Vec<String>,
    /// Whether `## Child DOX Index` currently contains the unindexed project sentinel.
    pub has_unindexed_sentinel: bool,
    /// Additional H2 sections preserved verbatim in order `(heading_line, body_lines)`.
    pub extra_sections: Vec<(String, Vec<String>)>,
}

impl DoxDocument {
    /// Creates a new canonical `DoxDocument` with the 6-section `agent0ai/dox` shape.
    #[must_use]
    pub fn new(rel_path: &str, scope_dir: &str, role: DoxRole, title: &str) -> Self {
        Self {
            rel_path: normalize_rel_path(rel_path),
            scope_dir: normalize_rel_path(scope_dir),
            role,
            title: title.to_string(),
            preamble: Vec::new(),
            purpose: Vec::new(),
            ownership: Vec::new(),
            local_contracts: Vec::new(),
            work_guidance: Vec::new(),
            verification: Vec::new(),
            user_preferences: Vec::new(),
            child_index: Vec::new(),
            parent_owned_items: Vec::new(),
            has_unindexed_sentinel: false,
            extra_sections: Vec::new(),
        }
    }

    /// Parses any Markdown DOX file (`minicode.md`, `AGENTS.md`, `.dox`, `RULES.md`) into a `DoxDocument`.
    #[must_use]
    pub fn parse(rel_path: &str, raw_content: &str) -> Self {
        let norm_rel = normalize_rel_path(rel_path);
        let scope_dir = derive_scope_dir(&norm_rel);
        let role = if scope_dir.is_empty() {
            DoxRole::RootRail
        } else {
            DoxRole::ChildContract
        };

        let mut doc = Self::new(
            &norm_rel,
            &scope_dir,
            role,
            if scope_dir.is_empty() {
                "DOX Root Contract"
            } else {
                &scope_dir
            },
        );

        let mut current_h2: Option<String> = None;
        let mut current_lines: Vec<String> = Vec::new();
        let mut seen_h1 = false;

        for line in raw_content.lines() {
            let trimmed = line.trim();
            if !seen_h1 && trimmed.starts_with("# ") && !trimmed.starts_with("## ") {
                doc.title = trimmed.trim_start_matches("# ").trim().to_string();
                seen_h1 = true;
                continue;
            }
            if trimmed.starts_with("## ") && !trimmed.starts_with("### ") {
                if let Some(prev_h2) = current_h2.take() {
                    doc.assign_section(&prev_h2, std::mem::take(&mut current_lines));
                } else if !current_lines.is_empty() {
                    doc.preamble = trim_blank_edges(std::mem::take(&mut current_lines));
                }
                current_h2 = Some(trimmed.to_string());
            } else {
                current_lines.push(line.to_string());
            }
        }

        if let Some(last_h2) = current_h2.take() {
            doc.assign_section(&last_h2, current_lines);
        } else if !current_lines.is_empty() {
            doc.preamble = trim_blank_edges(current_lines);
        }

        doc
    }

    fn assign_section(&mut self, heading_line: &str, lines: Vec<String>) {
        let cleaned = trim_blank_edges(lines);
        let heading_title = heading_line
            .trim_start_matches('#')
            .trim()
            .to_ascii_lowercase();

        match Self::classify_section_heading(&heading_title) {
            Some(DoxSectionArchetype::Purpose) => {
                if self.purpose.is_empty() {
                    self.purpose = cleaned;
                } else {
                    self.extra_sections
                        .push((heading_line.to_string(), cleaned));
                }
            }
            Some(DoxSectionArchetype::Ownership) => {
                if self.ownership.is_empty() {
                    self.ownership = cleaned;
                } else {
                    self.extra_sections
                        .push((heading_line.to_string(), cleaned));
                }
            }
            Some(DoxSectionArchetype::LocalContracts) => {
                if self.local_contracts.is_empty() {
                    self.local_contracts = cleaned;
                } else {
                    self.extra_sections
                        .push((heading_line.to_string(), cleaned));
                }
            }
            Some(DoxSectionArchetype::WorkGuidance) => {
                if self.work_guidance.is_empty() {
                    self.work_guidance = cleaned;
                } else {
                    self.extra_sections
                        .push((heading_line.to_string(), cleaned));
                }
            }
            Some(DoxSectionArchetype::Verification) => {
                if self.verification.is_empty() {
                    self.verification = cleaned;
                } else {
                    self.extra_sections
                        .push((heading_line.to_string(), cleaned));
                }
            }
            Some(DoxSectionArchetype::UserPreferences) => {
                self.user_preferences = cleaned;
            }
            Some(DoxSectionArchetype::ChildDoxIndex) => {
                self.parse_child_index_lines(&cleaned);
            }
            None => {
                self.extra_sections
                    .push((heading_line.to_string(), cleaned));
            }
        }
    }

    /// Classifies a Markdown H2 heading into one of the 7 canonical `agent0ai/dox` section archetypes
    /// using weighted token-stem similarity scoring instead of rigid string equality.
    fn classify_section_heading(title: &str) -> Option<DoxSectionArchetype> {
        const ARCHETYPES: &[(DoxSectionArchetype, &[(&str, usize)])] = &[
            (
                DoxSectionArchetype::ChildDoxIndex,
                &[
                    ("child", 4),
                    ("dox", 4),
                    ("index", 5),
                    ("subdirector", 5),
                    ("rout", 3),
                ],
            ),
            (
                DoxSectionArchetype::Purpose,
                &[("purpos", 6), ("summary", 5), ("overview", 4), ("role", 3)],
            ),
            (
                DoxSectionArchetype::Ownership,
                &[
                    ("ownership", 6),
                    ("owner", 5),
                    ("owned", 5),
                    ("scope", 3),
                    ("boundar", 3),
                ],
            ),
            (
                DoxSectionArchetype::LocalContracts,
                &[
                    ("contract", 6),
                    ("invariant", 6),
                    ("architectur", 4),
                    ("convention", 4),
                    ("rule", 3),
                ],
            ),
            (
                DoxSectionArchetype::WorkGuidance,
                &[
                    ("guidanc", 6),
                    ("guidelin", 6),
                    ("style", 4),
                    ("workflow", 4),
                    ("pattern", 3),
                ],
            ),
            (
                DoxSectionArchetype::Verification,
                &[
                    ("verif", 6),
                    ("command", 5),
                    ("check", 5),
                    ("test", 5),
                    ("essential", 3),
                    ("validate", 4),
                ],
            ),
            (
                DoxSectionArchetype::UserPreferences,
                &[("preferenc", 6), ("user", 3)],
            ),
        ];

        let words: Vec<&str> = title
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 3)
            .collect();
        if words.is_empty() {
            return None;
        }

        let mut best: Option<(DoxSectionArchetype, usize)> = None;
        for (archetype, stems) in ARCHETYPES {
            let mut score = 0usize;
            for word in &words {
                for (stem, weight) in *stems {
                    if word.starts_with(stem) || stem.starts_with(word) {
                        score += *weight;
                    }
                }
            }
            if score >= 4 && best.is_none_or(|(_, best_score)| score > best_score) {
                best = Some((*archetype, score));
            }
        }
        best.map(|(a, _)| a)
    }

    fn parse_child_index_lines(&mut self, lines: &[String]) {
        let joined_lower = lines.join("\n").to_ascii_lowercase();
        if joined_lower.contains(UNINDEXED_DOX_SENTINEL) {
            self.has_unindexed_sentinel = true;
        }

        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed
                .to_ascii_lowercase()
                .contains(UNINDEXED_DOX_SENTINEL)
            {
                self.has_unindexed_sentinel = true;
                continue;
            }
            let bullet = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .unwrap_or(trimmed);

            if bullet.to_ascii_lowercase().starts_with("parent-owned:")
                || bullet.to_ascii_lowercase().starts_with("root-owned:")
                || bullet.to_ascii_lowercase().starts_with("owned here:")
            {
                self.parent_owned_items.push(bullet.to_string());
                continue;
            }

            if let Some(entry) = parse_single_child_index_bullet(&self.scope_dir, bullet) {
                if !self
                    .child_index
                    .iter()
                    .any(|e| e.scope_prefix == entry.scope_prefix)
                {
                    self.child_index.push(entry);
                }
            } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                self.parent_owned_items.push(bullet.to_string());
            }
        }
    }

    /// Renders the document in the canonical `agent0ai/dox` section order:
    /// `Purpose` -> `Ownership` -> `Local Contracts` -> `Work Guidance` -> `Verification` ->
    /// (`User Preferences` if present) -> (`Extra Sections` if present) -> `Child DOX Index`.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n", self.title.trim()));

        if !self.preamble.is_empty() {
            out.push('\n');
            out.push_str(&self.preamble.join("\n"));
            out.push('\n');
        }

        // 1. Purpose
        out.push_str("\n## Purpose\n");
        if !self.purpose.is_empty() {
            out.push_str(&self.purpose.join("\n"));
            out.push('\n');
        }

        // 2. Ownership
        out.push_str("\n## Ownership\n");
        if !self.ownership.is_empty() {
            out.push_str(&self.ownership.join("\n"));
            out.push('\n');
        }

        // 3. Local Contracts
        out.push_str("\n## Local Contracts\n");
        if !self.local_contracts.is_empty() {
            out.push_str(&self.local_contracts.join("\n"));
            out.push('\n');
        }

        // 4. Work Guidance (left empty if no specific standards yet per agent0ai/dox spec)
        out.push_str("\n## Work Guidance\n");
        if !self.work_guidance.is_empty() {
            out.push_str(&self.work_guidance.join("\n"));
            out.push('\n');
        }

        // 5. Verification (left empty if no verification check exists yet per agent0ai/dox spec)
        out.push_str("\n## Verification\n");
        if !self.verification.is_empty() {
            out.push_str(&self.verification.join("\n"));
            out.push('\n');
        }

        // Optional User Preferences
        if !self.user_preferences.is_empty() || self.role == DoxRole::RootRail {
            out.push_str("\n## User Preferences\n");
            if !self.user_preferences.is_empty() {
                out.push_str(&self.user_preferences.join("\n"));
                out.push('\n');
            }
        }

        // Preserve any extra sections (such as harvested rules blocks)
        for (heading, lines) in &self.extra_sections {
            if lines.is_empty() {
                continue;
            }
            out.push('\n');
            out.push_str(heading);
            out.push('\n');
            out.push_str(&lines.join("\n"));
            out.push('\n');
        }

        // 6. Child DOX Index (always at the bottom per agent0ai/dox Default section order)
        out.push_str("\n## Child DOX Index\n");
        if self.child_index.is_empty() && self.parent_owned_items.is_empty() {
            out.push_str("- No child DOX files required; all files in this subtree are governed directly by this contract.\n");
        } else {
            for item in &self.parent_owned_items {
                out.push_str(&format!("- {}\n", item));
            }
            for entry in &self.child_index {
                if entry.summary.is_empty() {
                    out.push_str(&format!(
                        "- `{}` (scope: `{}/**`)\n",
                        entry.child_doc_path, entry.scope_prefix
                    ));
                } else {
                    out.push_str(&format!(
                        "- `{}` (scope: `{}/**`) — {}\n",
                        entry.child_doc_path, entry.scope_prefix, entry.summary
                    ));
                }
            }
        }

        out
    }

    /// Extracts executable verification commands from `## Verification`.
    #[must_use]
    pub fn extract_verification_commands(&self) -> Vec<String> {
        let mut cmds = Vec::new();
        let mut in_code_block = false;

        for line in &self.verification {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    cmds.push(trimmed.to_string());
                }
                continue;
            }
            // Extract backticked commands on bullet lines
            let mut rest = trimmed;
            while let Some(start) = rest.find('`') {
                let after = &rest[start + 1..];
                if let Some(end) = after.find('`') {
                    let candidate = after[..end].trim();
                    if looks_like_verification_command(candidate)
                        && !cmds.iter().any(|c| c == candidate)
                    {
                        cmds.push(candidate.to_string());
                    }
                    rest = &after[end + 1..];
                } else {
                    break;
                }
            }
        }
        cmds
    }

    /// Returns a concise one-line purpose summary suitable for a parent's `## Child DOX Index`.
    #[must_use]
    pub fn concise_summary(&self) -> String {
        for line in &self.purpose {
            let clean = line
                .trim()
                .trim_start_matches("- ")
                .trim_start_matches("* ")
                .trim();
            if !clean.is_empty() && !clean.starts_with("<!--") {
                let single: String = clean.chars().take(120).collect();
                return single;
            }
        }
        for (_, lines) in &self.extra_sections {
            for line in lines {
                let clean = line
                    .trim()
                    .trim_start_matches("- ")
                    .trim_start_matches("* ")
                    .trim();
                if !clean.is_empty() && !clean.starts_with('#') && !clean.starts_with("<!--") {
                    let single: String = clean.chars().take(120).collect();
                    return single;
                }
            }
        }
        format!("Local DOX contract for `{}`", self.scope_dir)
    }
}

/// Report generated by the 6-Step `agent0ai/dox` Closeout & Post-Edit Drift Auditor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoxCloseoutReport {
    /// Touch chain of existing DOX files from Root Rail down to Nearest Local Contract(s).
    pub touched_dox_chain: Vec<String>,
    /// Nearest owning DOX file for each changed path `(changed_path, owning_dox_path)`.
    pub path_owners: Vec<(String, String)>,
    /// DOX files whose `## Ownership` or `## Local Contracts` may need review due to new/deleted files.
    pub drifted_dox_files: Vec<(String, String)>,
    /// DOX files that can remain unchanged with explicit rationale (Closeout Step 6).
    pub unchanged_dox_rationale: Vec<(String, String)>,
    /// Subtree verification commands extracted from the touched DOX chain (Closeout Step 5).
    pub verification_commands: Vec<String>,
    /// Number of parent `## Child DOX Index` sections automatically refreshed (Closeout Step 3).
    pub indexes_refreshed: usize,
}

impl DoxCloseoutReport {
    /// Formats a concise prompt block for `<dox_closeout_pass>` in recency context.
    #[must_use]
    pub fn to_prompt_block(&self) -> String {
        if self.path_owners.is_empty() && self.drifted_dox_files.is_empty() {
            return String::new();
        }
        let mut out = String::from("  <dox_closeout_pass>\n");
        if !self.touched_dox_chain.is_empty() {
            out.push_str(&format!(
                "    Active DOX Chain: {}\n",
                self.touched_dox_chain.join(" -> ")
            ));
        }
        if !self.drifted_dox_files.is_empty() {
            out.push_str("    DOX Contracts Needing Review/Update (Update After Editing):\n");
            for (doc, reason) in &self.drifted_dox_files {
                out.push_str(&format!("      • `{}`: {}\n", doc, reason));
            }
        }
        if !self.verification_commands.is_empty() {
            out.push_str(&format!(
                "    Subtree DOX Verification Checks: `{}`\n",
                self.verification_commands.join("`, `")
            ));
        }
        out.push_str("  </dox_closeout_pass>\n");
        out
    }

    /// Formats a detailed Markdown report for `power_verify` and `dox_tree(action="closeout")`.
    #[must_use]
    pub fn to_markdown_report(&self) -> String {
        let mut md = String::from("### 🧭 DOX 6-Step Closeout & Contract Audit\n");
        if self.touched_dox_chain.is_empty() {
            md.push_str("- **DOX Chain:** No DOX contracts found in workspace.\n");
            return md;
        }
        md.push_str(&format!(
            "- **Active DOX Chain:** `{}`\n",
            self.touched_dox_chain.join("` → `")
        ));
        md.push_str(&format!(
            "- **Child DOX Indexes Refreshed:** {}\n",
            self.indexes_refreshed
        ));

        if !self.drifted_dox_files.is_empty() {
            md.push_str("- **Contracts Flagged for Update:**\n");
            for (doc, reason) in &self.drifted_dox_files {
                md.push_str(&format!("  - ⚠️ `{}` — {}\n", doc, reason));
            }
        } else {
            md.push_str(
                "- **Contract Drift:** ✅ 0 structural drift findings across owning DOX contracts.\n",
            );
        }

        if !self.unchanged_dox_rationale.is_empty() {
            md.push_str("- **Docs Intentionally Left Unchanged (Closeout Step 6):**\n");
            for (doc, why) in &self.unchanged_dox_rationale {
                md.push_str(&format!("  - `{}` — {}\n", doc, why));
            }
        }

        if !self.verification_commands.is_empty() {
            md.push_str(&format!(
                "- **Subtree Verification Checks (Closeout Step 5):** `{}`\n",
                self.verification_commands.join("`, `")
            ));
        }

        md
    }
}

/// Normalizes a relative path (`\` -> `/`, strips leading `./` and trailing `/`).
#[must_use]
pub fn normalize_rel_path(path: &str) -> String {
    let replaced = path.replace('\\', "/");
    let stripped = replaced.trim().trim_start_matches("./").trim_matches('/');
    if stripped == "." {
        String::new()
    } else {
        stripped.to_string()
    }
}

/// Derives the directory scope governed by a DOX file's relative path.
/// E.g.:
/// - `.minicode/minicode.md` -> `""` (workspace root)
/// - `AGENTS.md` -> `""` (workspace root)
/// - `src/agent/.minicode/minicode.md` -> `"src/agent"`
/// - `src/agent/minicode.md` -> `"src/agent"`
#[must_use]
pub fn derive_scope_dir(rel_doc_path: &str) -> String {
    let norm = normalize_rel_path(rel_doc_path);
    let path = Path::new(&norm);
    let Some(parent) = path.parent() else {
        return String::new();
    };
    let parent_str = normalize_rel_path(&parent.display().to_string());
    if parent_str.is_empty() || parent_str == crate::constants::WORKSPACE_DIR_NAME {
        return String::new();
    }
    if let Some(stripped) =
        parent_str.strip_suffix(&format!("/{}", crate::constants::WORKSPACE_DIR_NAME))
    {
        return normalize_rel_path(stripped);
    }
    parent_str
}

fn trim_blank_edges(mut lines: Vec<String>) -> Vec<String> {
    while lines.first().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

fn looks_like_verification_command(s: &str) -> bool {
    let lower = s.trim().to_ascii_lowercase();
    if lower.is_empty() || !lower.contains(' ') {
        return false;
    }
    let prefixes = [
        "cargo ", "nice ", "npm ", "pnpm ", "yarn ", "bun ", "pytest", "python ", "uv ", "go test",
        "go vet", "make ", "just ", "gradle ", "mvn ", "dotnet ", "ctest", "bash ", "sh ",
    ];
    prefixes.iter().any(|p| lower.starts_with(p))
}

fn parse_single_child_index_bullet(parent_scope: &str, bullet: &str) -> Option<DoxChildIndexEntry> {
    // Supported formats:
    // 1. `path/to/minicode.md` (scope: `path/to/**`) — Summary
    // 2. [label](path/to/AGENTS.md) — Summary
    // 3. `path/to/AGENTS.md`: Summary
    let mut doc_path = None;
    if let Some(start) = bullet.find('`') {
        let after = &bullet[start + 1..];
        if let Some(end) = after.find('`') {
            let candidate = normalize_rel_path(&after[..end]);
            if is_dox_like_path(&candidate) || candidate.contains('/') {
                doc_path = Some(candidate);
            }
        }
    }
    if doc_path.is_none() {
        if let (Some(open_p), Some(close_p)) = (bullet.find("]("), bullet.find(')')) {
            if close_p > open_p + 2 {
                let link_target = normalize_rel_path(&bullet[open_p + 2..close_p]);
                if !link_target.starts_with("http") {
                    doc_path = Some(link_target);
                }
            }
        }
    }

    let raw_doc_path = doc_path?;
    let resolved_doc_path = if !parent_scope.is_empty()
        && !raw_doc_path.starts_with(parent_scope)
        && !Path::new(&raw_doc_path).is_absolute()
    {
        normalize_rel_path(&format!("{}/{}", parent_scope, raw_doc_path))
    } else {
        raw_doc_path
    };

    let scope_prefix = if resolved_doc_path.ends_with(".md") || resolved_doc_path.ends_with(".dox")
    {
        derive_scope_dir(&resolved_doc_path)
    } else {
        normalize_rel_path(resolved_doc_path.trim_end_matches("/**"))
    };

    if scope_prefix.is_empty() {
        return None;
    }

    let summary = bullet
        .split_once("—")
        .map(|(_, s)| s.trim())
        .or_else(|| bullet.split_once(" - ").map(|(_, s)| s.trim()))
        .or_else(|| bullet.split_once(": ").map(|(_, s)| s.trim()))
        .unwrap_or("")
        .to_string();

    let final_doc_path =
        if resolved_doc_path.ends_with(".md") || resolved_doc_path.ends_with(".dox") {
            resolved_doc_path
        } else {
            format!("{}/minicode.md", scope_prefix)
        };

    Some(DoxChildIndexEntry {
        child_doc_path: final_doc_path,
        scope_prefix,
        summary,
    })
}

fn is_dox_like_path(p: &str) -> bool {
    let lower = p.to_ascii_lowercase();
    lower.ends_with("minicode.md")
        || lower.ends_with("agents.md")
        || lower.ends_with("rules.md")
        || lower.ends_with(".dox")
        || lower.ends_with(".dox.md")
}

/// Discovers all canonical and foreign DOX files across the workspace (excluding `.git`, `target`, `node_modules`, `.minicode/worktrees`).
/// Returns a map from `scope_dir` (`""` for root, `"src/agent"` for subdir) to the primary DOX file path (relative to `workspace_root`).
#[must_use]
pub fn discover_workspace_dox_map(workspace_root: &Path) -> BTreeMap<String, PathBuf> {
    let mut scope_map: BTreeMap<String, (u8, PathBuf)> = BTreeMap::new();

    // Check root first
    register_dir_dox_candidates(workspace_root, workspace_root, &mut scope_map);

    let walker = WalkBuilder::new(workspace_root)
        .hidden(false)
        .git_ignore(true)
        .max_depth(Some(5))
        .filter_entry(|entry| {
            let name = entry.file_name().to_str().unwrap_or("");
            !matches!(
                name,
                ".git"
                    | "target"
                    | "node_modules"
                    | "dist"
                    | "build"
                    | ".next"
                    | "__pycache__"
                    | ".venv"
                    | "worktrees"
                    | "sessions"
                    | "plan"
                    | "sdd"
                    | "wiki"
            )
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_dir() && path != workspace_root {
            if path.file_name().and_then(|n| n.to_str())
                == Some(crate::constants::WORKSPACE_DIR_NAME)
            {
                continue;
            }
            register_dir_dox_candidates(workspace_root, path, &mut scope_map);
        }
    }

    scope_map
        .into_iter()
        .map(|(scope, (_, path))| (scope, path))
        .collect()
}

fn register_dir_dox_candidates(
    workspace_root: &Path,
    dir: &Path,
    scope_map: &mut BTreeMap<String, (u8, PathBuf)>,
) {
    let candidates: &[(&str, u8)] = &[
        (".minicode/minicode.md", 1),
        ("minicode.md", 2),
        (".dox.md", 3),
        (".dox", 4),
        (".minicode/dox.md", 5),
        (".minicode/RULES.md", 6),
        ("RULES.md", 7),
        ("AGENTS.md", 8),
    ];

    let scope_dir = dir
        .strip_prefix(workspace_root)
        .map(|p| normalize_rel_path(&p.display().to_string()))
        .unwrap_or_default();

    for &(rel_name, priority) in candidates {
        let full = dir.join(rel_name);
        if full.is_file() {
            let rel_from_root = full
                .strip_prefix(workspace_root)
                .map(|p| PathBuf::from(normalize_rel_path(&p.display().to_string())))
                .unwrap_or(full);
            match scope_map.get(&scope_dir) {
                Some((existing_prio, _)) if *existing_prio <= priority => {}
                _ => {
                    scope_map.insert(scope_dir.clone(), (priority, rel_from_root));
                }
            }
            break;
        }
    }
}

/// Refreshes the `## Child DOX Index` in every existing DOX file across the workspace bottom-up.
/// Replaces any `"This project is not yet indexed..."` sentinel with the actual child index and parent-owned files.
/// Returns the number of DOX files whose `## Child DOX Index` was updated.
pub fn refresh_all_child_dox_indexes(workspace_root: &Path) -> usize {
    let scope_map = discover_workspace_dox_map(workspace_root);
    if scope_map.is_empty() {
        return 0;
    }

    // Parse all discovered DOX documents so we can extract their concise summaries
    let mut parsed_docs: BTreeMap<String, DoxDocument> = BTreeMap::new();
    for (scope, rel_path) in &scope_map {
        let full_path = workspace_root.join(rel_path);
        if let Ok(raw) = fs::read_to_string(&full_path) {
            let rel_str = rel_path.display().to_string();
            parsed_docs.insert(scope.clone(), DoxDocument::parse(&rel_str, &raw));
        }
    }

    // Process bottom-up (longest scope_dir first, root `""` last)
    let mut scopes_desc: Vec<String> = scope_map.keys().cloned().collect();
    scopes_desc.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));

    let mut updated_count = 0usize;

    for parent_scope in &scopes_desc {
        // Find direct children of `parent_scope`:
        // A scope `child_scope` is a direct DOX child of `parent_scope` if `parent_scope` is the
        // longest proper ancestor scope of `child_scope` in `scope_map`.
        let mut direct_children = Vec::new();
        for (candidate_scope, candidate_rel_path) in &scope_map {
            if candidate_scope == parent_scope {
                continue;
            }
            if is_direct_dox_child(parent_scope, candidate_scope, &scope_map) {
                let summary = parsed_docs
                    .get(candidate_scope)
                    .map(|d| d.concise_summary())
                    .unwrap_or_else(|| format!("Scoped DOX contract for `{}`", candidate_scope));
                direct_children.push(DoxChildIndexEntry {
                    child_doc_path: candidate_rel_path.display().to_string(),
                    scope_prefix: candidate_scope.clone(),
                    summary,
                });
            }
        }

        let Some(rel_doc_path) = scope_map.get(parent_scope) else {
            continue;
        };
        let full_doc_path = workspace_root.join(rel_doc_path);
        let Ok(raw_content) = fs::read_to_string(&full_doc_path) else {
            continue;
        };

        let parent_dir = if parent_scope.is_empty() {
            workspace_root.to_path_buf()
        } else {
            workspace_root.join(parent_scope)
        };
        let owned_summary =
            summarize_parent_owned_files(&parent_dir, parent_scope, &direct_children);

        if let Some(updated_content) =
            upsert_child_dox_index_section(&raw_content, &direct_children, &owned_summary)
        {
            if updated_content != raw_content && fs::write(&full_doc_path, &updated_content).is_ok()
            {
                updated_count += 1;
            }
        }
    }

    updated_count
}

fn is_direct_dox_child(
    parent_scope: &str,
    candidate_scope: &str,
    all_scopes: &BTreeMap<String, PathBuf>,
) -> bool {
    if parent_scope.is_empty() {
        if candidate_scope.is_empty() {
            return false;
        }
    } else {
        let prefix = format!("{}/", parent_scope);
        if !candidate_scope.starts_with(&prefix) {
            return false;
        }
    }

    // Ensure no intermediate scope exists between `parent_scope` and `candidate_scope`
    for other_scope in all_scopes.keys() {
        if other_scope == parent_scope || other_scope == candidate_scope || other_scope.is_empty() {
            continue;
        }
        let under_parent = if parent_scope.is_empty() {
            true
        } else {
            other_scope.starts_with(&format!("{}/", parent_scope))
        };
        let above_candidate = candidate_scope.starts_with(&format!("{}/", other_scope));
        if under_parent && above_candidate {
            return false;
        }
    }
    true
}

fn summarize_parent_owned_files(
    dir: &Path,
    scope_dir: &str,
    direct_children: &[DoxChildIndexEntry],
) -> String {
    let Ok(entries) = fs::read_dir(dir) else {
        return String::new();
    };
    let mut files = Vec::new();
    let mut dirs = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || matches!(
                name.as_str(),
                "target" | "node_modules" | "dist" | "build" | "__pycache__"
            )
        {
            continue;
        }
        let child_rel = if scope_dir.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", scope_dir, name)
        };
        if direct_children.iter().any(|c| {
            c.scope_prefix == child_rel || c.scope_prefix.starts_with(&format!("{}/", child_rel))
        }) {
            continue;
        }
        if entry.path().is_dir() {
            dirs.push(format!("`{}/`", name));
        } else if entry.path().is_file() {
            files.push(format!("`{}`", name));
        }
    }

    files.sort();
    dirs.sort();
    let mut combined = files;
    combined.extend(dirs);
    if combined.is_empty() {
        String::new()
    } else {
        let shown: Vec<String> = combined.into_iter().take(10).collect();
        format!("Parent-owned in this scope: {}", shown.join(", "))
    }
}

fn upsert_child_dox_index_section(
    raw_content: &str,
    direct_children: &[DoxChildIndexEntry],
    owned_summary: &str,
) -> Option<String> {
    let lower = raw_content.to_ascii_lowercase();
    let has_sentinel = lower.contains(UNINDEXED_DOX_SENTINEL);
    let has_index_heading = raw_content
        .lines()
        .any(|l| l.trim().eq_ignore_ascii_case("## Child DOX Index"));

    if !has_sentinel && !has_index_heading && direct_children.is_empty() {
        return None;
    }

    let mut new_index_block = String::from("## Child DOX Index\n");
    if !owned_summary.is_empty() {
        new_index_block.push_str(&format!("- {}\n", owned_summary));
    }
    if direct_children.is_empty() {
        new_index_block.push_str(
            "- No child DOX files needed; all files in this scope are governed directly by this contract.\n",
        );
    } else {
        for child in direct_children {
            if child.summary.is_empty() {
                new_index_block.push_str(&format!(
                    "- `{}` (scope: `{}/**`)\n",
                    child.child_doc_path, child.scope_prefix
                ));
            } else {
                new_index_block.push_str(&format!(
                    "- `{}` (scope: `{}/**`) — {}\n",
                    child.child_doc_path, child.scope_prefix, child.summary
                ));
            }
        }
    }

    if has_index_heading {
        let mut out_lines = Vec::new();
        let mut in_index = false;
        let mut replaced = false;

        for line in raw_content.lines() {
            let trimmed = line.trim();
            if trimmed.eq_ignore_ascii_case("## Child DOX Index") {
                in_index = true;
                if !replaced {
                    out_lines.push(new_index_block.trim_end().to_string());
                    replaced = true;
                }
                continue;
            }
            if in_index && trimmed.starts_with("## ") && !trimmed.starts_with("### ") {
                in_index = false;
            }
            if !in_index {
                out_lines.push(line.to_string());
            }
        }
        let mut final_str = out_lines.join("\n");
        if !final_str.ends_with('\n') {
            final_str.push('\n');
        }
        Some(final_str)
    } else {
        let mut final_str = raw_content.trim_end().to_string();
        final_str.push_str("\n\n");
        final_str.push_str(&new_index_block);
        Some(final_str)
    }
}

/// Initializes or refreshes the hierarchical DOX tree across the workspace in native Rust:
/// 1. Scans the repository using `ignore::WalkBuilder` to identify durable architectural boundaries
///    (sub-packages, crates, or multi-file domain directories).
/// 2. Creates canonical 6-section child DOX contracts (`<dir>/.minicode/minicode.md` or `<dir>/minicode.md`)
///    for durable boundaries that do not yet have one (or when `scaffold_children` is true).
/// 3. Ensures the root DOX Rail (`.minicode/minicode.md`) exists and refreshes all `## Child DOX Index`
///    sections bottom-up, replacing any `"This project is not yet indexed..."` sentinel.
pub fn initialize_dox_tree(
    workspace_root: &Path,
    scaffold_children: bool,
    max_depth: usize,
) -> Vec<String> {
    let mut created_or_updated = Vec::new();
    let depth_limit = max_depth.clamp(1, 4);

    // 1. Discover durable boundaries if scaffold_children is enabled
    if scaffold_children {
        let boundaries = detect_durable_boundaries(workspace_root, depth_limit);
        let existing_map = discover_workspace_dox_map(workspace_root);

        // Determine if workspace uses root AGENTS.md or .minicode/minicode.md convention
        let root_has_agents_md = workspace_root.join("AGENTS.md").is_file()
            && !workspace_root.join(".minicode/minicode.md").is_file();

        for boundary_rel in boundaries {
            if existing_map.contains_key(&boundary_rel) {
                continue;
            }
            let boundary_dir = workspace_root.join(&boundary_rel);
            let doc = synthesize_child_dox_contract(workspace_root, &boundary_dir, &boundary_rel);
            let target_rel = if root_has_agents_md {
                format!("{}/minicode.md", boundary_rel)
            } else {
                format!("{}/.minicode/minicode.md", boundary_rel)
            };
            let target_full = workspace_root.join(&target_rel);
            if let Some(parent) = target_full.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::write(&target_full, doc.render()).is_ok() {
                created_or_updated.push(target_rel);
            }
        }
    }

    // 2. Ensure root DOX Rail exists
    let existing_after = discover_workspace_dox_map(workspace_root);
    if !existing_after.contains_key("") {
        let root_doc = synthesize_child_dox_contract(workspace_root, workspace_root, "");
        let root_target = workspace_root.join(crate::constants::MINICODE_MD_FILE);
        if let Some(parent) = root_target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(&root_target, root_doc.render()).is_ok() {
            created_or_updated.push(crate::constants::MINICODE_MD_FILE.to_string());
        }
    }

    // 3. Refresh all Child DOX Indexes bottom-up (including replacing unindexed sentinel in AGENTS.md if present)
    let _ = refresh_all_child_dox_indexes(workspace_root);
    let root_agents = workspace_root.join("AGENTS.md");
    if root_agents.is_file() {
        if let Ok(raw) = fs::read_to_string(&root_agents) {
            if raw.to_ascii_lowercase().contains(UNINDEXED_DOX_SENTINEL) {
                let scope_map = discover_workspace_dox_map(workspace_root);
                let mut direct_children = Vec::new();
                for (scope, rel_path) in &scope_map {
                    if !scope.is_empty() && is_direct_dox_child("", scope, &scope_map) {
                        let summary = fs::read_to_string(workspace_root.join(rel_path))
                            .ok()
                            .map(|c| {
                                DoxDocument::parse(&rel_path.display().to_string(), &c)
                                    .concise_summary()
                            })
                            .unwrap_or_default();
                        direct_children.push(DoxChildIndexEntry {
                            child_doc_path: rel_path.display().to_string(),
                            scope_prefix: scope.clone(),
                            summary,
                        });
                    }
                }
                let owned = summarize_parent_owned_files(workspace_root, "", &direct_children);
                if let Some(updated) =
                    upsert_child_dox_index_section(&raw, &direct_children, &owned)
                {
                    let _ = fs::write(&root_agents, updated);
                }
            }
        }
    }

    created_or_updated
}

/// Detects directories that form a durable architectural boundary (own manifest, module entrypoint,
/// or cohesive multi-file domain subtree) up to `max_depth`.
#[must_use]
pub fn detect_durable_boundaries(workspace_root: &Path, max_depth: usize) -> Vec<String> {
    let mut candidates = BTreeSet::new();

    let walker = WalkBuilder::new(workspace_root)
        .hidden(true)
        .git_ignore(true)
        .max_depth(Some(max_depth))
        .filter_entry(|entry| {
            let name = entry.file_name().to_str().unwrap_or("");
            !matches!(
                name,
                ".git"
                    | ".minicode"
                    | "target"
                    | "node_modules"
                    | "dist"
                    | "build"
                    | "vendor"
                    | "coverage"
                    | "__pycache__"
                    | ".venv"
            )
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_dir() || path == workspace_root {
            continue;
        }
        let Ok(rel) = path.strip_prefix(workspace_root) else {
            continue;
        };
        let rel_str = normalize_rel_path(&rel.display().to_string());
        if rel_str.is_empty() {
            continue;
        }

        if is_durable_boundary_dir(path) {
            candidates.insert(rel_str);
        }
    }

    candidates.into_iter().collect()
}

fn is_durable_boundary_dir(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };

    let mut source_files = 0usize;
    let mut has_manifest_or_entry = false;
    let mut subdirs = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            subdirs += 1;
            continue;
        }
        if matches!(
            name.as_str(),
            "Cargo.toml"
                | "package.json"
                | "pyproject.toml"
                | "go.mod"
                | "mod.rs"
                | "lib.rs"
                | "main.rs"
                | "index.ts"
                | "index.tsx"
                | "index.js"
                | "__init__.py"
        ) {
            has_manifest_or_entry = true;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext,
                "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "zig"
            ) {
                source_files += 1;
            }
        }
    }

    // A directory is a durable boundary if it has a package/module entrypoint with >= 2 source files
    // or is a multi-file domain module (>= 3 source files or >= 2 source files + subdirs).
    (has_manifest_or_entry && (source_files >= 2 || subdirs >= 1))
        || source_files >= 3
        || (source_files >= 2 && subdirs >= 1)
}

fn synthesize_child_dox_contract(
    workspace_root: &Path,
    dir: &Path,
    scope_dir: &str,
) -> DoxDocument {
    let role = if scope_dir.is_empty() {
        DoxRole::RootRail
    } else {
        DoxRole::ChildContract
    };
    let rel_doc = if scope_dir.is_empty() {
        crate::constants::MINICODE_MD_FILE.to_string()
    } else {
        format!("{}/.minicode/minicode.md", scope_dir)
    };
    let title = if scope_dir.is_empty() {
        "DOX Root Contract".to_string()
    } else {
        format!("DOX Contract — `{}`", scope_dir)
    };

    let mut doc = DoxDocument::new(&rel_doc, scope_dir, role, &title);

    // Collect owned source files & module doc comments
    let mut owned_files = Vec::new();
    let mut doc_comment_summary = None;

    if let Ok(entries) = fs::read_dir(dir) {
        let mut names = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.')
                || matches!(name.as_str(), "target" | "node_modules" | "dist" | "build")
            {
                continue;
            }
            if path.is_file() {
                if doc_comment_summary.is_none()
                    && matches!(
                        name.as_str(),
                        "mod.rs" | "lib.rs" | "main.rs" | "README.md" | "__init__.py" | "index.ts"
                    )
                {
                    doc_comment_summary = extract_first_module_doc(&path);
                }
                names.push(name);
            }
        }
        names.sort();
        for name in names.into_iter().take(15) {
            let path_label = if scope_dir.is_empty() {
                format!("- `{}`", name)
            } else {
                format!("- `{}/{}`", scope_dir, name)
            };
            owned_files.push(path_label);
        }
    }

    let purpose_line = doc_comment_summary.unwrap_or_else(|| {
        if scope_dir.is_empty() {
            "- Root DOX rail governing project-wide engineering contracts, workflow invariants, and top-level Child DOX Index.".to_string()
        } else {
            format!(
                "- Durable module boundary governing `{}` domain logic and interfaces.",
                scope_dir
            )
        }
    });
    doc.purpose.push(purpose_line);
    doc.ownership = owned_files;
    doc.local_contracts.push(if scope_dir.is_empty() {
        "- Nearest child DOX contract controls local work details; root DOX rail governs repository-wide invariants.".to_string()
    } else {
        format!(
            "- Maintain clean module boundaries within `{}` and keep public interfaces synchronized with callers.",
            scope_dir
        )
    });

    // Detect verification command from workspace root or local manifest
    if let Some(verify_cmd) = detect_verification_command(workspace_root, dir) {
        doc.verification.push(format!("- `{}`", verify_cmd));
    }

    doc
}

/// Extracts a concise module summary using a multi-syntax docstring state machine
/// (`//!`, `///`, `/** ... */`, `/*! ... */`, Python `"""..."""`) with a Tree-sitter AST
/// symbol extraction fallback when no top-of-file docstring comment is present.
fn extract_first_module_doc(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_block_doc = false;
    let mut in_py_doc: Option<&str> = None;

    for line in content.lines().take(35) {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("#!")
            || trimmed.starts_with("# -*-")
            || trimmed.starts_with("// @")
        {
            continue;
        }

        // Active JSDoc / C / Rust block docstring state (`/** ... */` or `/*! ... */`)
        if in_block_doc {
            if trimmed.starts_with("*/") {
                break;
            }
            let clean = trimmed
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .trim();
            if !clean.is_empty() && !clean.starts_with('@') {
                return Some(format!("- {}", clean));
            }
            if trimmed.ends_with("*/") {
                break;
            }
            continue;
        }

        // Active Python triple-quoted module docstring state
        if let Some(delim) = in_py_doc {
            let clean = trimmed.trim_end_matches(delim).trim();
            if !clean.is_empty() {
                return Some(format!("- {}", clean));
            }
            if trimmed.ends_with(delim) {
                break;
            }
            continue;
        }

        // 1. Line docstrings (`//!` or `///`)
        if let Some(doc) = trimmed
            .strip_prefix("//!")
            .or_else(|| trimmed.strip_prefix("///"))
        {
            let clean = doc.trim();
            if !clean.is_empty() {
                return Some(format!("- {}", clean));
            }
            continue;
        }

        // 2. Block docstring start (`/**` or `/*!`)
        if let Some(rest) = trimmed
            .strip_prefix("/**")
            .or_else(|| trimmed.strip_prefix("/*!"))
        {
            let inline = rest.trim_end_matches("*/").trim_start_matches('*').trim();
            if !inline.is_empty() && !inline.starts_with('@') {
                return Some(format!("- {}", inline));
            }
            if !rest.ends_with("*/") {
                in_block_doc = true;
            }
            continue;
        }

        // 3. Python module docstring (`"""` or `'''`)
        for delim in ["\"\"\"", "'''"] {
            if let Some(rest) = trimmed.strip_prefix(delim) {
                let inline = rest.trim_end_matches(delim).trim();
                if !inline.is_empty() {
                    return Some(format!("- {}", inline));
                }
                if !rest.ends_with(delim) {
                    in_py_doc = Some(delim);
                }
                break;
            }
        }
        if in_py_doc.is_some() {
            continue;
        }

        // Stop scanning header once code statements start
        if !trimmed.starts_with("//") && !trimmed.starts_with('#') {
            break;
        }
    }

    // 4. Fallback: Tree-sitter AST symbol summary from `RepoMapExtractor`
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let extractor = crate::context::ast::repomap::RepoMapExtractor::new();
        if let Ok(symbols) = extractor.extract_symbols_from_str(ext, &content) {
            let mut names = Vec::new();
            for sym in symbols {
                if sym.kind != "import" && !sym.name.is_empty() && !names.contains(&sym.name) {
                    if let Some(doc) = sym.doc_comment {
                        let first_sentence = doc.lines().next().unwrap_or_default().trim();
                        if !first_sentence.is_empty() {
                            return Some(format!("- {}", first_sentence));
                        }
                    }
                    names.push(sym.name);
                    if names.len() >= 4 {
                        break;
                    }
                }
            }
            if !names.is_empty() {
                return Some(format!(
                    "- Defines primary AST symbols: `{}`.",
                    names.join("`, `")
                ));
            }
        }
    }

    None
}

/// Dynamically inspects local and workspace project manifests (`Cargo.toml`, `package.json`,
/// `Makefile`, `pyproject.toml`, `go.mod`) to synthesize an accurate verification command.
fn detect_verification_command(workspace_root: &Path, dir: &Path) -> Option<String> {
    // 1. Rust Cargo manifest (with package-scoped check when inside a workspace member crate)
    let local_cargo = dir.join("Cargo.toml");
    if local_cargo.is_file() {
        if dir != workspace_root {
            if let Ok(raw) = fs::read_to_string(&local_cargo) {
                if let Some(pkg_name) = parse_toml_package_name(&raw) {
                    return Some(format!("nice -n 19 cargo check -p {} -j 1", pkg_name));
                }
            }
        }
        return Some("nice -n 19 cargo check -j 1".to_string());
    }
    if workspace_root.join("Cargo.toml").is_file() {
        return Some("nice -n 19 cargo check -j 1".to_string());
    }

    // 2. Node/TS/Bun package.json scripts + lockfile runner resolution
    for scope in [dir, workspace_root] {
        let pkg_path = scope.join("package.json");
        if pkg_path.is_file() {
            let runner = if scope.join("bun.lockb").is_file()
                || scope.join("bun.lock").is_file()
                || workspace_root.join("bun.lockb").is_file()
                || workspace_root.join("bun.lock").is_file()
            {
                "bun run"
            } else if scope.join("pnpm-lock.yaml").is_file()
                || workspace_root.join("pnpm-lock.yaml").is_file()
            {
                "pnpm"
            } else if scope.join("yarn.lock").is_file()
                || workspace_root.join("yarn.lock").is_file()
            {
                "yarn"
            } else {
                "npm run"
            };

            if let Ok(raw) = fs::read_to_string(&pkg_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(scripts) = val.get("scripts").and_then(|s| s.as_object()) {
                        for preferred in ["check", "typecheck", "test", "lint", "build"] {
                            if scripts.contains_key(preferred) {
                                return Some(format!("{} {}", runner, preferred));
                            }
                        }
                    }
                }
            }
            return Some(format!("{} test", runner));
        }
    }

    // 3. Makefile targets
    for scope in [dir, workspace_root] {
        let makefile = scope.join("Makefile");
        if makefile.is_file() {
            if let Ok(raw) = fs::read_to_string(&makefile) {
                for preferred in ["check", "test", "verify", "lint"] {
                    let prefix = format!("{}:", preferred);
                    if raw.lines().any(|l| l.trim_start().starts_with(&prefix)) {
                        return Some(format!("make {}", preferred));
                    }
                }
            }
        }
    }

    // 4. Python pyproject.toml
    for scope in [dir, workspace_root] {
        let pyproj = scope.join("pyproject.toml");
        if pyproj.is_file() {
            if let Ok(raw) = fs::read_to_string(&pyproj) {
                if raw.contains("[tool.pytest") || raw.contains("pytest") {
                    return Some("pytest".to_string());
                }
                if raw.contains("[tool.ruff") {
                    return Some("ruff check .".to_string());
                }
            }
            return Some("pytest".to_string());
        }
    }

    // 5. Go module
    if dir.join("go.mod").is_file() || workspace_root.join("go.mod").is_file() {
        return Some("go test ./...".to_string());
    }

    None
}

fn parse_toml_package_name(raw_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in raw_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package {
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == "name" {
                    let name = v.trim().trim_matches('"').trim_matches('\'');
                    if !name.is_empty() {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Resolves `## Child DOX Index` scope routing:
/// Given a parent DOX document and a set of active relative paths, returns any child DOX file paths
/// explicitly referenced in the parent's `## Child DOX Index` whose `scope_prefix` covers an active path.
#[must_use]
pub fn route_via_child_dox_index(
    workspace_root: &Path,
    parent_doc_content: &str,
    parent_rel_path: &str,
    active_rel_paths: &[String],
) -> Vec<PathBuf> {
    let parsed = DoxDocument::parse(parent_rel_path, parent_doc_content);
    let mut routed = Vec::new();

    for entry in &parsed.child_index {
        let covers_active = active_rel_paths.iter().any(|active| {
            let norm = normalize_rel_path(active);
            norm == entry.scope_prefix || norm.starts_with(&format!("{}/", entry.scope_prefix))
        });
        if covers_active {
            let child_full = workspace_root.join(&entry.child_doc_path);
            if child_full.is_file() {
                routed.push(child_full);
            }
        }
    }
    routed
}

/// Runs the 6-Step `agent0ai/dox` Closeout & Post-Edit Drift Pass over the given changed files:
/// 1. Re-checks changed paths against the DOX chain.
/// 2. Identifies nearest owning DOX docs and detects ownership/contract drift.
/// 3. Refreshes affected `## Child DOX Index` sections bottom-up.
/// 4. Collects subtree `## Verification` commands.
/// 5. Records rationale for DOX docs intentionally left unchanged.
pub fn execute_dox_closeout_pass(
    workspace_root: &Path,
    changed_files: &[impl AsRef<Path>],
    refresh_indexes: bool,
) -> DoxCloseoutReport {
    let indexes_refreshed = if refresh_indexes {
        refresh_all_child_dox_indexes(workspace_root)
    } else {
        0
    };

    let scope_map = discover_workspace_dox_map(workspace_root);
    let mut report = DoxCloseoutReport {
        indexes_refreshed,
        ..Default::default()
    };

    if scope_map.is_empty() {
        return report;
    }

    let mut touched_scopes = BTreeSet::new();
    touched_scopes.insert(String::new()); // Root Rail is always part of the chain

    let mut modified_dox_paths = HashSet::new();
    for f in changed_files {
        let rel = f
            .as_ref()
            .strip_prefix(workspace_root)
            .unwrap_or(f.as_ref())
            .display()
            .to_string();
        let norm = normalize_rel_path(&rel);
        if is_dox_like_path(&norm) {
            modified_dox_paths.insert(norm);
        }
    }

    let mut owner_to_touched_files: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for file in changed_files {
        let rel = file
            .as_ref()
            .strip_prefix(workspace_root)
            .unwrap_or(file.as_ref())
            .display()
            .to_string();
        let norm = normalize_rel_path(&rel);
        if norm.is_empty()
            || norm.starts_with(".minicode/sessions")
            || norm.starts_with(".minicode/plan")
        {
            continue;
        }

        // Find all ancestor DOX scopes and the nearest owning DOX scope
        let mut best_scope = String::new();
        for scope in scope_map.keys() {
            if scope.is_empty() {
                continue;
            }
            if norm == *scope || norm.starts_with(&format!("{}/", scope)) {
                touched_scopes.insert(scope.clone());
                if scope.len() > best_scope.len() {
                    best_scope = scope.clone();
                }
            }
        }

        if let Some(owner_rel_path) = scope_map.get(&best_scope) {
            let owner_str = owner_rel_path.display().to_string();
            report.path_owners.push((norm.clone(), owner_str.clone()));
            owner_to_touched_files
                .entry(best_scope)
                .or_default()
                .push(norm);
        }
    }

    // Build ordered touched chain (Root -> Child)
    let mut ordered_scopes: Vec<String> = touched_scopes.into_iter().collect();
    ordered_scopes.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

    for scope in &ordered_scopes {
        if let Some(rel_doc) = scope_map.get(scope) {
            let rel_doc_str = rel_doc.display().to_string();
            report.touched_dox_chain.push(rel_doc_str.clone());

            let full_doc = workspace_root.join(rel_doc);
            if let Ok(raw) = fs::read_to_string(&full_doc) {
                let parsed = DoxDocument::parse(&rel_doc_str, &raw);
                for cmd in parsed.extract_verification_commands() {
                    if !report.verification_commands.contains(&cmd) {
                        report.verification_commands.push(cmd);
                    }
                }

                if parsed.has_unindexed_sentinel {
                    report.drifted_dox_files.push((
                        rel_doc_str.clone(),
                        "Contains unindexed DOX sentinel (`This project is not yet indexed`); run `dox_tree(action=\"index\")`.".to_string(),
                    ));
                    continue;
                }

                // Check if any touched files owned by this DOX doc are missing from a non-empty `## Ownership` section
                if let Some(files_for_scope) = owner_to_touched_files.get(scope) {
                    if !parsed.ownership.is_empty() && !modified_dox_paths.contains(&rel_doc_str) {
                        let ownership_joined = parsed.ownership.join("\n");
                        let unlisted: Vec<String> = files_for_scope
                            .iter()
                            .filter(|f| !is_dox_like_path(f))
                            .filter(|f| {
                                let basename = Path::new(f.as_str())
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or(f.as_str());
                                !ownership_joined.contains(f.as_str())
                                    && !ownership_joined.contains(basename)
                            })
                            .cloned()
                            .collect();

                        if !unlisted.is_empty() {
                            report.drifted_dox_files.push((
                                rel_doc_str.clone(),
                                format!(
                                    "Touched file(s) `{}` not listed in `## Ownership`; update if durable structure/contract changed.",
                                    unlisted.join("`, `")
                                ),
                            ));
                        } else {
                            report.unchanged_dox_rationale.push((
                                rel_doc_str,
                                "Internal edits within existing ownership & local contracts; no contract drift.".to_string(),
                            ));
                        }
                    } else if !modified_dox_paths.contains(&rel_doc_str) {
                        report.unchanged_dox_rationale.push((
                            rel_doc_str,
                            "Edits conform to existing subtree contract; no boundary or workflow changes.".to_string(),
                        ));
                    }
                }
            }
        }
    }

    report
}

/// Records a durable user preference into the root DOX Rail (`## User Preferences`) or a scoped
/// child DOX contract (`## Work Guidance`), deduplicating in place.
pub fn record_user_preference_in_dox(
    workspace_root: &Path,
    scope_dir: Option<&str>,
    preference: &str,
) -> std::io::Result<PathBuf> {
    let clean_pref = preference
        .trim()
        .trim_start_matches("- ")
        .trim_start_matches("* ")
        .trim();
    if clean_pref.is_empty() {
        return Ok(workspace_root.join(crate::constants::MINICODE_MD_FILE));
    }

    let bullet = format!("- {}", clean_pref);
    let target_scope = scope_dir.map(normalize_rel_path).unwrap_or_default();
    let scope_map = discover_workspace_dox_map(workspace_root);

    let target_rel = scope_map.get(&target_scope).cloned().unwrap_or_else(|| {
        if target_scope.is_empty() {
            PathBuf::from(crate::constants::MINICODE_MD_FILE)
        } else {
            PathBuf::from(format!("{}/.minicode/minicode.md", target_scope))
        }
    });
    let target_full = workspace_root.join(&target_rel);
    if let Some(parent) = target_full.parent() {
        fs::create_dir_all(parent)?;
    }

    let section_header = if target_scope.is_empty() {
        "## User Preferences"
    } else {
        "## Work Guidance"
    };

    let existing = fs::read_to_string(&target_full)
        .unwrap_or_else(|_| format!("# DOX Contract\n\n{}\n", section_header));

    if existing.contains(clean_pref) {
        return Ok(target_full);
    }

    let updated = if existing
        .lines()
        .any(|l| l.trim().eq_ignore_ascii_case(section_header))
    {
        let mut lines_out = Vec::new();
        let mut inserted = false;
        for line in existing.lines() {
            lines_out.push(line.to_string());
            if !inserted && line.trim().eq_ignore_ascii_case(section_header) {
                lines_out.push(bullet.clone());
                inserted = true;
            }
        }
        let mut s = lines_out.join("\n");
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s
    } else {
        let mut s = existing.trim_end().to_string();
        s.push_str(&format!("\n\n{}\n{}\n", section_header, bullet));
        s
    };

    fs::write(&target_full, updated)?;
    Ok(target_full)
}
