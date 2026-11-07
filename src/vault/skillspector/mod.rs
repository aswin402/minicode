//! SkillSpector: Algorithmic Static Security, Taint-Flow, Entropy & Least-Privilege Auditor for Agent Skills.
//!
//! Inspired by `NVIDIA/skillspector`, engineered natively in Rust using:
//! - **Tree-sitter CST/AST & `petgraph` Intraprocedural Taint Reachability** (`ast_taint.rs`)
//! - **Information-Theoretic Shannon Entropy $H(X)$, Passive Image Magic & Unicode Script-Set Automaton** (`entropy_and_unicode.rs`)
//! - **`pulldown-cmark` Markdown AST, Least-Privilege Set Algebra ($U \setminus D$), Token Intent Vectors & JSON/Manifest AST** (`semantic_and_privilege.rs`)

pub mod ast_taint;
pub mod entropy_and_unicode;
pub mod semantic_and_privilege;

use crate::constants::{
    FNV_OFFSET_BASIS, FNV_PRIME, SKILLSPECTOR_BASELINE_FILE_NAME, SKILLSPECTOR_CAUTION_MAX_SCORE,
    SKILLSPECTOR_HIGH_MAX_SCORE, SKILLSPECTOR_INGEST_MAX_BYTES, SKILLSPECTOR_MARKDOWN_EXTENSIONS,
    SKILLSPECTOR_MAX_FILE_BYTES, SKILLSPECTOR_MAX_RISK_SCORE, SKILLSPECTOR_MAX_WALK_DEPTH,
    SKILLSPECTOR_SAFE_MAX_SCORE, SKILLSPECTOR_SCRIPT_EXTENSIONS,
    SKILLSPECTOR_SCRIPT_RISK_MULTIPLIER, SKILLSPECTOR_WEIGHT_CRITICAL, SKILLSPECTOR_WEIGHT_HIGH,
    SKILLSPECTOR_WEIGHT_LOW, SKILLSPECTOR_WEIGHT_MEDIUM,
};
use crate::vault::builtin::get_all_builtin_skills;
use crate::vault::models::{parse_skill_markdown, SkillScope, VaultSkill};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Severity level of a SkillSpector security finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FindingSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl FindingSeverity {
    #[must_use]
    pub const fn weight(self) -> u32 {
        match self {
            Self::Low => SKILLSPECTOR_WEIGHT_LOW,
            Self::Medium => SKILLSPECTOR_WEIGHT_MEDIUM,
            Self::High => SKILLSPECTOR_WEIGHT_HIGH,
            Self::Critical => SKILLSPECTOR_WEIGHT_CRITICAL,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    #[must_use]
    pub const fn sarif_level(self) -> &'static str {
        match self {
            Self::Low => "note",
            Self::Medium => "warning",
            Self::High | Self::Critical => "error",
        }
    }
}

/// The 17 security & quality taxonomy categories modeled after NVIDIA SkillSpector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCategory {
    PromptInjection,
    AntiRefusal,
    ToolPoisoning,
    DataExfiltration,
    PrivilegeEscalation,
    SupplyChain,
    RogueAgent,
    ExcessiveAgency,
    MemoryPoisoning,
    CredentialExposure,
    SandboxEscape,
    Ssrf,
    AstCodeAnalysis,
    TaintTracking,
    LeastPrivilege,
    TriggerQuality,
    ResourceAbuse,
}

impl FindingCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PromptInjection => "prompt_injection",
            Self::AntiRefusal => "anti_refusal",
            Self::ToolPoisoning => "tool_poisoning",
            Self::DataExfiltration => "data_exfiltration",
            Self::PrivilegeEscalation => "privilege_escalation",
            Self::SupplyChain => "supply_chain",
            Self::RogueAgent => "rogue_agent",
            Self::ExcessiveAgency => "excessive_agency",
            Self::MemoryPoisoning => "memory_poisoning",
            Self::CredentialExposure => "credential_exposure",
            Self::SandboxEscape => "sandbox_escape",
            Self::Ssrf => "ssrf",
            Self::AstCodeAnalysis => "ast_code_analysis",
            Self::TaintTracking => "taint_tracking",
            Self::LeastPrivilege => "least_privilege",
            Self::TriggerQuality => "trigger_quality",
            Self::ResourceAbuse => "resource_abuse",
        }
    }
}

/// Capabilities exercised by a skill's instructions or bundled scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExercisedCapability {
    Read,
    Write,
    Exec,
    Network,
    EnvRead,
    Wildcard,
}

impl ExercisedCapability {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Exec => "exec",
            Self::Network => "network",
            Self::EnvRead => "env_read",
            Self::Wildcard => "wildcard",
        }
    }
}

/// Installation verdict derived from the 0..=100 risk score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstallRecommendation {
    Safe,
    Caution,
    DoNotInstall,
}

impl InstallRecommendation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::Caution => "CAUTION",
            Self::DoNotInstall => "DO_NOT_INSTALL",
        }
    }
}

/// Declarative risk-score band table (`(max_score_inclusive, overall_severity, recommendation, safe_to_install)`).
const RISK_BAND_TABLE: &[(u8, FindingSeverity, InstallRecommendation, bool)] = &[
    (
        SKILLSPECTOR_SAFE_MAX_SCORE,
        FindingSeverity::Low,
        InstallRecommendation::Safe,
        true,
    ),
    (
        SKILLSPECTOR_CAUTION_MAX_SCORE,
        FindingSeverity::Medium,
        InstallRecommendation::Caution,
        true,
    ),
    (
        SKILLSPECTOR_HIGH_MAX_SCORE,
        FindingSeverity::High,
        InstallRecommendation::DoNotInstall,
        false,
    ),
    (
        SKILLSPECTOR_MAX_RISK_SCORE,
        FindingSeverity::Critical,
        InstallRecommendation::DoNotInstall,
        false,
    ),
];

/// Classification of an inspected artifact inside the per-file `InspectionLedgerEntry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Markdown,
    ExecutableScript,
    SvgVector,
    StructuredJson,
    DependencyManifest,
    PassiveImage,
    ReferenceText,
    OpaqueBinary,
}

/// Per-file inspection ledger entry (`inspection_ledger.py`) proving scan completeness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionLedgerEntry {
    pub file: String,
    pub kind: ArtifactKind,
    pub bytes: u64,
    pub inspected: bool,
}

/// Inventory of files, byte sizes, and inspection ledger inside a scanned skill bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentInventory {
    pub markdown_files: usize,
    pub executable_scripts: usize,
    pub reference_files: usize,
    pub asset_files: usize,
    pub total_bytes: u64,
    pub files_scanned: Vec<String>,
    #[serde(default)]
    pub ledger: Vec<InspectionLedgerEntry>,
}

/// A single security or quality finding emitted by SkillSpector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillFinding {
    pub id: String,
    pub category: FindingCategory,
    pub severity: FindingSeverity,
    pub confidence: f64,
    pub file: String,
    pub line: usize,
    pub title: String,
    pub description: String,
    pub evidence: String,
    pub remediation: String,
    pub fingerprint: String,
}

impl SkillFinding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        category: FindingCategory,
        severity: FindingSeverity,
        confidence: f64,
        file: &str,
        line: usize,
        title: &str,
        description: &str,
        evidence: String,
        remediation: &str,
    ) -> Self {
        let fingerprint = compute_evidence_fingerprint(id, file, &evidence);
        Self {
            id: id.to_string(),
            category,
            severity,
            confidence,
            file: file.to_string(),
            line,
            title: title.to_string(),
            description: description.to_string(),
            evidence,
            remediation: remediation.to_string(),
            fingerprint,
        }
    }
}

/// Computes a deterministic 64-bit FNV-1a hex fingerprint over `(rule_id, file, normalized_evidence)`.
#[must_use]
pub fn compute_evidence_fingerprint(rule_id: &str, file: &str, evidence: &str) -> String {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in rule_id
        .bytes()
        .chain(std::iter::once(b':'))
        .chain(file.bytes())
        .chain(std::iter::once(b':'))
        .chain(evidence.trim().bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("sp-{:016x}", hash)
}

/// Persistent baseline of reviewed/suppressed finding fingerprints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillSpectorBaseline {
    pub suppressed_fingerprints: HashSet<String>,
}

impl SkillSpectorBaseline {
    pub fn load_for_workspace(workspace_root: &Path) -> Self {
        let path = workspace_root
            .join(".minicode")
            .join("vault")
            .join(SKILLSPECTOR_BASELINE_FILE_NAME);
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save_for_workspace(&self, workspace_root: &Path) -> std::io::Result<()> {
        let dir = workspace_root.join(".minicode").join("vault");
        fs::create_dir_all(&dir)?;
        let path = dir.join(SKILLSPECTOR_BASELINE_FILE_NAME);
        let json = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        fs::write(path, json)
    }
}

/// Complete SkillSpector static & taint analysis report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillScanReport {
    pub skill_name: String,
    pub target_path: String,
    pub risk_score: u8,
    pub overall_severity: FindingSeverity,
    pub recommendation: InstallRecommendation,
    pub safe_to_install: bool,
    pub inventory: ComponentInventory,
    pub findings: Vec<SkillFinding>,
    pub suppressed_count: usize,
}

impl SkillScanReport {
    #[must_use]
    pub fn from_raw_findings(
        skill_name: String,
        target_path: String,
        inventory: ComponentInventory,
        raw_findings: Vec<SkillFinding>,
        baseline: Option<&SkillSpectorBaseline>,
    ) -> Self {
        let mut seen_keys = HashSet::new();
        let mut findings = Vec::new();
        let mut suppressed_count = 0usize;

        for f in raw_findings {
            if baseline.is_some_and(|base| base.suppressed_fingerprints.contains(&f.fingerprint)) {
                suppressed_count += 1;
                continue;
            }
            let dedup_key = (f.id.clone(), f.file.clone(), f.line);
            if seen_keys.insert(dedup_key) {
                findings.push(f);
            }
        }

        // Sort findings by severity descending, then file and line ascending
        findings.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });

        let base_score_sum: u32 = findings.iter().map(|f| f.severity.weight()).sum();
        let multiplier = if inventory.executable_scripts > 0 {
            SKILLSPECTOR_SCRIPT_RISK_MULTIPLIER
        } else {
            1.0
        };
        let scaled = (f64::from(base_score_sum) * multiplier + 0.5).floor() as u32;
        let risk_score = scaled.min(u32::from(SKILLSPECTOR_MAX_RISK_SCORE)) as u8;

        let (_, overall_severity, recommendation, safe_to_install) = RISK_BAND_TABLE
            .iter()
            .copied()
            .find(|(max_score, ..)| risk_score <= *max_score)
            .unwrap_or((
                SKILLSPECTOR_MAX_RISK_SCORE,
                FindingSeverity::Critical,
                InstallRecommendation::DoNotInstall,
                false,
            ));

        Self {
            skill_name,
            target_path,
            risk_score,
            overall_severity,
            recommendation,
            safe_to_install,
            inventory,
            findings,
            suppressed_count,
        }
    }

    /// Formats the report as a clean Markdown summary table and finding breakdown.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let badge = match self.recommendation {
            InstallRecommendation::Safe => "✅ SAFE TO INSTALL",
            InstallRecommendation::Caution => "⚠️ CAUTION (REVIEW FINDINGS)",
            InstallRecommendation::DoNotInstall => "🛑 DO NOT INSTALL (BLOCKED)",
        };

        let mut out = format!(
            "🛡️ **SkillSpector Security Audit: `{}`**\n\
             • **Verdict**: **{}** (`recommendation: {}`, `safe_to_install: {}`)\n\
             • **Risk Score**: **`{}/100`** (Severity: `{}` | Script Multiplier: `{}`)\n\
             • **Inventory**: `{}` markdown, `{}` executable script(s), `{}` reference(s), `{}` asset(s), `{}` total bytes\n",
            self.skill_name,
            badge,
            self.recommendation.as_str(),
            self.safe_to_install,
            self.risk_score,
            self.overall_severity.as_str(),
            if self.inventory.executable_scripts > 0 {
                "1.3x"
            } else {
                "1.0x"
            },
            self.inventory.markdown_files,
            self.inventory.executable_scripts,
            self.inventory.reference_files,
            self.inventory.asset_files,
            self.inventory.total_bytes
        );

        if self.suppressed_count > 0 {
            out.push_str(&format!(
                "• **Baseline Suppressed**: `{}` finding(s)\n",
                self.suppressed_count
            ));
        }

        if self.findings.is_empty() {
            out.push_str(
                "\n✔ **Zero security, taint-flow, entropy, or least-privilege violations detected.**\n",
            );
            return out;
        }

        out.push_str(&format!(
            "\n### Findings ({} active)\n\n| Rule | Severity | Category | Location | Title & Evidence |\n| :--- | :--- | :--- | :--- | :--- |\n",
            self.findings.len()
        ));

        for f in &self.findings {
            out.push_str(&format!(
                "| `{}` | **`{}`** | `{}` | `{}:{}` | **{}** — `{}` *(fingerprint: `{}`)* |\n",
                f.id,
                f.severity.as_str(),
                f.category.as_str(),
                f.file,
                f.line,
                f.title,
                f.evidence.replace('|', "\\|"),
                f.fingerprint
            ));
        }

        out
    }

    /// Formats the report as pretty-printed JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Formats the report in OASIS SARIF v2.1.0 format for CI/CD code scanning pipelines.
    #[must_use]
    pub fn to_sarif(&self) -> String {
        let results: Vec<serde_json::Value> = self
            .findings
            .iter()
            .map(|f| {
                json!({
                    "ruleId": f.id,
                    "level": f.severity.sarif_level(),
                    "message": {
                        "text": format!("[{}] {}: {} (Evidence: {})", f.category.as_str(), f.title, f.description, f.evidence)
                    },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": f.file },
                            "region": { "startLine": f.line.max(1) }
                        }
                    }],
                    "partialFingerprints": {
                        "skillspectorEvidenceHash": f.fingerprint
                    }
                })
            })
            .collect();

        let sarif = json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "minicode-skillspector",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/aswin402/minicode"
                    }
                },
                "properties": {
                    "skillName": self.skill_name,
                    "riskScore": self.risk_score,
                    "recommendation": self.recommendation.as_str(),
                    "safeToInstall": self.safe_to_install
                },
                "results": results
            }]
        });

        serde_json::to_string_pretty(&sarif).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Classifies an artifact deterministically from its relative path, extension, and header bytes.
fn classify_artifact(rel_path: &str, ext: &str, raw_bytes: &[u8]) -> ArtifactKind {
    let file_name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    if entropy_and_unicode::is_verified_passive_image(rel_path, raw_bytes) {
        return ArtifactKind::PassiveImage;
    }
    if SKILLSPECTOR_MARKDOWN_EXTENSIONS.contains(&ext) {
        return ArtifactKind::Markdown;
    }
    if ext == "svg" {
        return ArtifactKind::SvgVector;
    }
    if ext == "json" {
        return ArtifactKind::StructuredJson;
    }
    if matches!(
        file_name,
        "requirements.txt" | "pyproject.toml" | "Pipfile" | "Cargo.toml"
    ) {
        return ArtifactKind::DependencyManifest;
    }
    if SKILLSPECTOR_SCRIPT_EXTENSIONS.contains(&ext) || rel_path.starts_with("scripts/") {
        return ArtifactKind::ExecutableScript;
    }
    ArtifactKind::ReferenceText
}

/// Stateless algorithmic analyzer for auditing skills in memory or on disk.
pub struct SkillSpectorEngine;

impl SkillSpectorEngine {
    fn builtin_names_set() -> HashSet<String> {
        get_all_builtin_skills()
            .into_iter()
            .map(|s| s.name.to_ascii_lowercase())
            .collect()
    }

    /// Audits an in-memory markdown payload before writing a skill to disk.
    #[must_use]
    pub fn scan_raw_markdown(
        skill_name: &str,
        rel_path: &str,
        raw_content: &str,
        scope: SkillScope,
    ) -> SkillScanReport {
        let total_bytes = raw_content.len() as u64;
        let inventory = ComponentInventory {
            markdown_files: 1,
            total_bytes,
            files_scanned: vec![rel_path.to_string()],
            ledger: vec![InspectionLedgerEntry {
                file: rel_path.to_string(),
                kind: ArtifactKind::Markdown,
                bytes: total_bytes,
                inspected: total_bytes <= SKILLSPECTOR_MAX_FILE_BYTES,
            }],
            ..Default::default()
        };
        let mut findings = Vec::new();

        if total_bytes > SKILLSPECTOR_MAX_FILE_BYTES {
            findings.push(SkillFinding::new(
                "RA_SIZE",
                FindingCategory::ResourceAbuse,
                FindingSeverity::Critical,
                1.0,
                rel_path,
                1,
                "Skill File Exceeds Maximum Safe Size Bound (1 MiB)",
                "File exceeds SKILLSPECTOR_MAX_FILE_BYTES (1,048,576 bytes), failing closed against resource exhaustion.",
                format!("bytes={}", total_bytes),
                "Split large reference data or reduce file size below 1 MiB.",
            ));
            return SkillScanReport::from_raw_findings(
                skill_name.to_string(),
                rel_path.to_string(),
                inventory,
                findings,
                None,
            );
        }

        findings.extend(entropy_and_unicode::analyze_binary_payload(
            rel_path,
            raw_content.as_bytes(),
        ));
        findings.extend(entropy_and_unicode::analyze_text_entropy_and_unicode(
            rel_path,
            raw_content,
        ));

        let (fm, _) = parse_skill_markdown(raw_content, skill_name);
        let (md_findings, md_caps) =
            semantic_and_privilege::analyze_markdown_structure_and_intent(rel_path, raw_content);
        findings.extend(md_findings);

        let builtin_names = Self::builtin_names_set();
        findings.extend(semantic_and_privilege::analyze_privilege_and_triggers(
            rel_path,
            &fm,
            scope,
            false,
            &md_caps,
            &builtin_names,
        ));

        SkillScanReport::from_raw_findings(
            skill_name.to_string(),
            rel_path.to_string(),
            inventory,
            findings,
            None,
        )
    }

    /// Audits a parsed `VaultSkill` along with any directory-backed Level-3 resources (`scripts/`, `references/`, etc.).
    #[must_use]
    pub fn scan_vault_skill(
        skill: &VaultSkill,
        baseline: Option<&SkillSpectorBaseline>,
    ) -> SkillScanReport {
        if let Some(ref skill_path) = skill.path {
            if skill_path.file_name().and_then(|s| s.to_str()) == Some("SKILL.md") {
                if let Some(skill_dir) = skill_path.parent() {
                    if skill_dir.is_dir() {
                        return Self::scan_directory_or_file_with_scope(
                            skill_dir,
                            Some(&skill.name),
                            skill.scope,
                            baseline,
                        );
                    }
                }
            }
            if skill_path.is_file() {
                return Self::scan_directory_or_file_with_scope(
                    skill_path,
                    Some(&skill.name),
                    skill.scope,
                    baseline,
                );
            }
        }

        let mut report =
            Self::scan_raw_markdown(&skill.name, "SKILL.md", &skill.raw_content, skill.scope);
        if let Some(base) = baseline {
            report = SkillScanReport::from_raw_findings(
                report.skill_name,
                report.target_path,
                report.inventory,
                report.findings,
                Some(base),
            );
        }
        report
    }

    /// Audits an arbitrary skill directory or `.md` / script file on disk.
    #[must_use]
    pub fn scan_directory_or_file(
        path: &Path,
        baseline: Option<&SkillSpectorBaseline>,
    ) -> SkillScanReport {
        Self::scan_directory_or_file_with_scope(path, None, SkillScope::Project, baseline)
    }

    #[must_use]
    pub fn scan_directory_or_file_with_scope(
        path: &Path,
        name_override: Option<&str>,
        scope: SkillScope,
        baseline: Option<&SkillSpectorBaseline>,
    ) -> SkillScanReport {
        let derived_name = name_override
            .map(str::to_string)
            .or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .filter(|s| !s.eq_ignore_ascii_case("SKILL"))
                    .map(str::to_string)
            })
            .or_else(|| {
                path.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "scanned-skill".to_string());

        let mut inventory = ComponentInventory::default();
        let mut findings = Vec::new();
        let mut exercised_caps = HashSet::new();
        let mut parsed_frontmatter = None;

        let files_to_inspect: Vec<std::path::PathBuf> = if path.is_file() {
            vec![path.to_path_buf()]
        } else if path.is_dir() {
            ignore::WalkBuilder::new(path)
                .hidden(false)
                .git_ignore(true)
                .max_depth(Some(SKILLSPECTOR_MAX_WALK_DEPTH))
                .build()
                .flatten()
                .map(|e| e.into_path())
                .filter(|p| {
                    p.is_file()
                        && !p
                            .components()
                            .any(|c| c.as_os_str() == ".git" || c.as_os_str() == ".DS_Store")
                })
                .collect()
        } else {
            Vec::new()
        };

        let base_dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };

        for file_path in files_to_inspect {
            let rel_path = file_path
                .strip_prefix(base_dir)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .replace('\\', "/");

            let Ok(meta) = fs::metadata(&file_path) else {
                continue;
            };
            let file_len = meta.len();
            inventory.total_bytes = inventory.total_bytes.saturating_add(file_len);
            inventory.files_scanned.push(rel_path.clone());

            if inventory.total_bytes > SKILLSPECTOR_INGEST_MAX_BYTES {
                findings.push(SkillFinding::new(
                    "RA_BUNDLE_SIZE",
                    FindingCategory::ResourceAbuse,
                    FindingSeverity::Critical,
                    1.0,
                    &rel_path,
                    1,
                    "Skill Bundle Exceeds 100 MiB Ingest Cap",
                    "Total bundle size exceeds SKILLSPECTOR_INGEST_MAX_BYTES (104,857,600 bytes); failing closed.",
                    format!("total_bytes={}", inventory.total_bytes),
                    "Remove large binary blobs or datasets from the skill bundle.",
                ));
                break;
            }

            if file_len > SKILLSPECTOR_MAX_FILE_BYTES {
                inventory.ledger.push(InspectionLedgerEntry {
                    file: rel_path.clone(),
                    kind: ArtifactKind::OpaqueBinary,
                    bytes: file_len,
                    inspected: false,
                });
                findings.push(SkillFinding::new(
                    "RA_FILE_SIZE",
                    FindingCategory::ResourceAbuse,
                    FindingSeverity::Critical,
                    1.0,
                    &rel_path,
                    1,
                    "Skill File Exceeds 1 MiB Analysis Cap",
                    "Individual file exceeds SKILLSPECTOR_MAX_FILE_BYTES (1,048,576 bytes); failing closed.",
                    format!("file_bytes={}", file_len),
                    "Keep individual skill files and scripts under 1 MiB.",
                ));
                continue;
            }

            let Ok(raw_bytes) = fs::read(&file_path) else {
                continue;
            };

            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            let artifact_kind = classify_artifact(&rel_path, &ext, &raw_bytes);

            match artifact_kind {
                ArtifactKind::PassiveImage => {
                    inventory.asset_files += 1;
                    inventory.ledger.push(InspectionLedgerEntry {
                        file: rel_path,
                        kind: ArtifactKind::PassiveImage,
                        bytes: file_len,
                        inspected: true,
                    });
                    continue;
                }
                ArtifactKind::Markdown => inventory.markdown_files += 1,
                ArtifactKind::ExecutableScript => inventory.executable_scripts += 1,
                ArtifactKind::SvgVector => inventory.asset_files += 1,
                ArtifactKind::StructuredJson
                | ArtifactKind::DependencyManifest
                | ArtifactKind::ReferenceText
                | ArtifactKind::OpaqueBinary => {
                    if rel_path.starts_with("references/") || rel_path.starts_with("reference/") {
                        inventory.reference_files += 1;
                    } else {
                        inventory.asset_files += 1;
                    }
                }
            }

            let bin_findings = entropy_and_unicode::analyze_binary_payload(&rel_path, &raw_bytes);
            if !bin_findings.is_empty() {
                inventory.ledger.push(InspectionLedgerEntry {
                    file: rel_path,
                    kind: ArtifactKind::OpaqueBinary,
                    bytes: file_len,
                    inspected: true,
                });
                findings.extend(bin_findings);
                continue;
            }

            let Ok(text) = String::from_utf8(raw_bytes) else {
                continue;
            };

            inventory.ledger.push(InspectionLedgerEntry {
                file: rel_path.clone(),
                kind: artifact_kind,
                bytes: file_len,
                inspected: true,
            });

            findings.extend(entropy_and_unicode::analyze_text_entropy_and_unicode(
                &rel_path, &text,
            ));

            match artifact_kind {
                ArtifactKind::Markdown => {
                    if parsed_frontmatter.is_none()
                        || rel_path.eq_ignore_ascii_case("SKILL.md")
                        || rel_path.ends_with("/SKILL.md")
                    {
                        let (fm, _) = parse_skill_markdown(&text, &derived_name);
                        parsed_frontmatter = Some((rel_path.clone(), fm));
                    }
                    let (md_findings, md_caps) =
                        semantic_and_privilege::analyze_markdown_structure_and_intent(
                            &rel_path, &text,
                        );
                    findings.extend(md_findings);
                    exercised_caps.extend(md_caps);
                }
                ArtifactKind::ExecutableScript => {
                    let script_tag = if ext.is_empty() { "sh" } else { ext.as_str() };
                    let (ast_findings, ast_caps) =
                        ast_taint::analyze_script_ast_and_taint(&rel_path, script_tag, &text, 0);
                    findings.extend(ast_findings);
                    exercised_caps.extend(ast_caps);
                }
                ArtifactKind::SvgVector => {
                    findings.extend(semantic_and_privilege::analyze_svg_document(
                        &rel_path, &text,
                    ));
                }
                ArtifactKind::StructuredJson => {
                    findings.extend(semantic_and_privilege::analyze_mcp_and_package_json(
                        &rel_path, &text,
                    ));
                }
                ArtifactKind::DependencyManifest => {
                    findings.extend(semantic_and_privilege::analyze_dependency_manifest(
                        &rel_path, &text,
                    ));
                }
                ArtifactKind::PassiveImage
                | ArtifactKind::ReferenceText
                | ArtifactKind::OpaqueBinary => {}
            }
        }

        if let Some((fm_path, fm)) = parsed_frontmatter {
            let builtin_names = Self::builtin_names_set();
            findings.extend(semantic_and_privilege::analyze_privilege_and_triggers(
                &fm_path,
                &fm,
                scope,
                inventory.executable_scripts > 0,
                &exercised_caps,
                &builtin_names,
            ));
        }

        SkillScanReport::from_raw_findings(
            derived_name,
            path.display().to_string(),
            inventory,
            findings,
            baseline,
        )
    }
}
