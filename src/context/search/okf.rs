pub mod attestation;
pub mod index_log;
pub mod paths;
pub mod viewer;
pub mod viewer_assets;
pub mod web_enricher;
pub mod yaml;

pub use attestation::{
    attest_computation, attest_concept_doc, bind_parameters, canonicalize_sql,
    extract_computation_text, AttestationVerdict,
};
pub use index_log::{
    append_okf_log_entry, build_index_text, regenerate_indexes, synthesize_directory_description,
};
pub use paths::{
    concept_id_to_path, parse_concept_id, path_to_concept_id, read_existing_doc,
    schema_field_names, validate_segment, write_concept_doc, WriteConceptOutcome,
};
pub use viewer::{extract_links, generate_visualization, VisualizationStats};
pub use web_enricher::{validate_bundle_conformance, BundleValidationReport, OkfWebCrawlState};

use crate::error::{ContextError, Result};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Provenance source record in OKF v0.2 (§5.1)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OkfSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

/// Optional usage window bounding `sources[].usage_count` in OKF v0.2 (§5.1)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OkfUsageWindow {
    pub from: String,
    pub to: String,
}

/// Generation metadata record in OKF v0.2 (§5.2)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfGenerated {
    pub by: String,
    pub at: String,
}

/// Trust and verification record in OKF v0.2 (§5.2)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfVerification {
    pub by: String,
    pub at: String,
}

/// Typed parameter declaration for `type: Attested Computation` in OKF v0.2 (§10.2)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// Executor declaration for `type: Attested Computation` in OKF v0.2 (§10.2)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfExecutor {
    pub resource: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipt: Vec<String>,
}

/// Attester declaration for `type: Attested Computation` in OKF v0.2 (§10.2)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfAttester {
    pub resource: String,
}

/// YAML Frontmatter metadata container for OKF v0.2 concept documents (§4, §5, §10)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfFrontmatter {
    #[serde(rename = "type")]
    pub concept_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<OkfSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated: Option<OkfGenerated>,
    /// Primary verification event (when `verified` is a single mapping or first list item).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<OkfVerification>,
    /// Full normalized list of verification events (`verified` as mapping or list per §5.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verified_events: Vec<OkfVerification>,
    /// Lifecycle status (`draft | stable | deprecated`, default `"stable"` per §5.4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    /// Absolute ISO 8601 UTC instant on/after which content is stale (§5.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_after: Option<String>,
    /// Optional usage window for `sources[].usage_count` (§5.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_window: Option<OkfUsageWindow>,
    /// Required for `type: Attested Computation` (e.g., `bigquery`, `postgres`, `dbt`, `python`, `rust`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    /// Typed parameter definitions for `type: Attested Computation` (§10.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<OkfParameter>,
    /// Optional path to external computation file instead of inline `# Computation` (§10.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computation: Option<String>,
    /// Executor contract for `type: Attested Computation` (§10.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<OkfExecutor>,
    /// Deterministic attester contract for `type: Attested Computation` (§10.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attester: Option<OkfAttester>,
    /// Arbitrary producer-defined extension keys preserved across round-trips (§4.1).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub extensions: IndexMap<String, Value>,
}

impl Default for OkfFrontmatter {
    fn default() -> Self {
        Self {
            concept_type: "Concept".to_string(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            sources: Vec::new(),
            generated: Some(OkfGenerated {
                by: format!("minicode/v{}", env!("CARGO_PKG_VERSION")),
                at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            }),
            verified: None,
            verified_events: Vec::new(),
            status: Some("stable".to_string()),
            superseded_by: None,
            stale_after: None,
            usage_window: None,
            runtime: None,
            parameters: Vec::new(),
            computation: None,
            executor: None,
            attester: None,
            extensions: IndexMap::new(),
        }
    }
}

impl OkfFrontmatter {
    /// Validates required keys per OKF v0.2 §10.2 & §11 (`type` is always required; `runtime` is required for `Attested Computation`).
    pub fn validate(&self) -> Result<()> {
        if self.concept_type.trim().is_empty() {
            return Err(
                ContextError::Okf("Missing required frontmatter key: type".to_string()).into(),
            );
        }
        if self.concept_type == "Attested Computation"
            && self
                .runtime
                .as_deref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(ContextError::Okf(
                "Attested Computation requires a non-empty `runtime` key in frontmatter"
                    .to_string(),
            )
            .into());
        }
        Ok(())
    }
}

/// Returns the `verified` events as a normalized list (OKF v0.2 §5.2 `normalize_verified`).
/// A bare mapping `{ by, at }` is treated as a one-element list.
pub fn normalize_verified(fm: &OkfFrontmatter) -> Vec<OkfVerification> {
    if !fm.verified_events.is_empty() {
        return fm.verified_events.clone();
    }
    if let Some(v) = &fm.verified {
        if !v.by.trim().is_empty() {
            return vec![v.clone()];
        }
    }
    Vec::new()
}

/// Derives a concept's trust tier from `verified` per OKF v0.2 §5.3 (`bundle/document.py:trust_tier`):
/// - No `verified` key => `"unverified"`
/// - `verified` by non-`human:` actors only => `"machine-confirmed"`
/// - `verified` by a `human:<id>` actor => `"human-reviewed"`
pub fn trust_tier(fm: &OkfFrontmatter) -> &'static str {
    let events = normalize_verified(fm);
    if events.is_empty() {
        return "unverified";
    }
    for event in &events {
        if event.by.trim().starts_with("human:") {
            return "human-reviewed";
        }
    }
    "machine-confirmed"
}

/// Checks whether a concept is stale per `stale_after` (OKF v0.2 §5.5 `bundle/document.py:is_stale`).
///
/// A concept is stale when `now >= stale_after`. Returns `false` when `stale_after` is absent,
/// or is not an ISO 8601 datetime with an explicit UTC offset (a date-only string like `2026-12-31`
/// without `T` and timezone offset is ignored rather than guessed at).
pub fn is_stale(fm: &OkfFrontmatter, now: Option<DateTime<Utc>>) -> bool {
    let Some(raw) = fm.stale_after.as_deref() else {
        return false;
    };
    let trimmed = raw.trim();
    if !trimmed.contains('T') {
        return false;
    }
    let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) else {
        return false;
    };
    let stale_after_utc = parsed.with_timezone(&Utc);
    let current = now.unwrap_or_else(Utc::now);
    current >= stale_after_utc
}

/// Parser and serializer for OKF v0.2 documents
pub struct OkfDocument;

impl OkfDocument {
    /// Parses an OKF document splitting YAML frontmatter and markdown body.
    pub fn parse(content: &str) -> Result<(Option<OkfFrontmatter>, &str)> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Ok((None, content));
        }

        let rest = &trimmed[3..];
        let end_idx = match rest.find("\n---") {
            Some(idx) => idx,
            None => return Ok((None, content)),
        };

        let yaml_str = rest[..end_idx].trim();
        let after_delim = &rest[end_idx + 4..];
        let body = after_delim
            .strip_prefix("\r\n")
            .or_else(|| after_delim.strip_prefix('\n'))
            .unwrap_or(after_delim)
            .trim_start_matches('\n');

        let frontmatter = match yaml::frontmatter_from_yaml_str(yaml_str, Some(body)) {
            Ok(fm) => fm,
            Err(_) => return Ok((None, content)),
        };

        Ok((Some(frontmatter), body))
    }

    /// Strict parser that returns `Err` on unterminated frontmatter or invalid YAML (`OKFDocument.parse`).
    pub fn parse_strict(content: &str) -> Result<(Option<OkfFrontmatter>, &str)> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Ok((None, content));
        }
        let rest = &trimmed[3..];
        let end_idx = rest
            .find("\n---")
            .ok_or_else(|| ContextError::Okf("Unterminated YAML frontmatter block".to_string()))?;

        let yaml_str = rest[..end_idx].trim();
        let after_delim = &rest[end_idx + 4..];
        let body = after_delim
            .strip_prefix("\r\n")
            .or_else(|| after_delim.strip_prefix('\n'))
            .unwrap_or(after_delim)
            .trim_start_matches('\n');

        let fm = yaml::frontmatter_from_yaml_str(yaml_str, Some(body))?;
        Ok((Some(fm), body))
    }

    /// Serializes an OKF frontmatter and body into compliant UTF-8 markdown.
    pub fn serialize(frontmatter: &OkfFrontmatter, body: &str) -> String {
        let yaml_str = yaml::frontmatter_to_yaml_string(frontmatter).unwrap_or_default();
        let trimmed_body = body.trim_start_matches('\n');
        let normalized_body = if trimmed_body.ends_with('\n') || trimmed_body.is_empty() {
            trimmed_body.to_string()
        } else {
            format!("{}\n", trimmed_body)
        };
        format!("---\n{}---\n\n{}", yaml_str, normalized_body)
    }
}

/// Manager for generating and maintaining OKF knowledge bundles and ledgers
pub struct OkfManager;

impl OkfManager {
    /// Recursively scans a bundle/docs directory bottom-up, generates `index.md` in every subdirectory
    /// and the bundle root per OKF v0.2 §8 & §12, and returns the root `index.md` content.
    pub fn generate_index_md(docs_dir: &Path) -> Result<String> {
        if !docs_dir.exists() {
            fs::create_dir_all(docs_dir).ok();
        }

        regenerate_indexes(docs_dir, true)?;

        let root_index_path = docs_dir.join("index.md");
        if root_index_path.exists() {
            Ok(fs::read_to_string(&root_index_path).unwrap_or_default())
        } else {
            let empty_index = "---\nokf_version: \"0.2\"\n---\n\n# Index\n";
            fs::write(&root_index_path, empty_index).ok();
            Ok(empty_index.to_string())
        }
    }

    /// Appends a chronological update record to `log.md` in OKF v0.2 §9 format (`## YYYY-MM-DD`).
    pub fn append_log_entry(
        docs_dir: &Path,
        actor: &str,
        action: &str,
        target_file: &str,
        summary: &str,
    ) -> Result<()> {
        append_okf_log_entry(docs_dir, actor, action, target_file, summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_okf_document_parse_and_serialize_full_v02() {
        let content = r#"---
type: Attested Computation
title: "Revenue for fiscal year"
description: "Recognized revenue for a fiscal year, per Finance's definition."
tags: [finance, revenue]
status: stable
runtime: bigquery
parameters:
  - { name: year, type: integer, required: true }
executor:
  resource: references/skills/run-on-bq.md
  receipt: [job_id, executed_sql, result]
attester:
  resource: references/attesters/sql-equality.py
generated: { by: "reference_agent/gemini-2.5-pro", at: "2026-06-28T14:00:00Z" }
verified: { by: "human:ahormati", at: "2026-06-25T09:00:00Z" }
stale_after: 2026-12-31T00:00:00Z
sources:
  - id: rev-policy
    resource: https://wiki.acme/finance/revenue-recognition
    title: "Revenue recognition policy"
    author: team:finance-fpa
    usage_count: 5000
    last_modified: 2026-04-02T00:00:00Z
usage_window: { from: 2026-06-01T00:00:00Z, to: 2026-06-30T00:00:00Z }
owner: team:finance
---

# Computation

    SELECT SUM(amount) AS revenue
    FROM finance.recognized_revenue
    WHERE fiscal_year = @year

Recognized revenue per the recognition policy.[^rev-policy]
"#;

        let (fm_opt, body) = OkfDocument::parse(content).unwrap();
        let fm = fm_opt.expect("frontmatter should parse");
        assert_eq!(fm.concept_type, "Attested Computation");
        assert_eq!(fm.title.as_deref(), Some("Revenue for fiscal year"));
        assert_eq!(fm.runtime.as_deref(), Some("bigquery"));
        assert_eq!(fm.parameters.len(), 1);
        assert_eq!(fm.parameters[0].name, "year");
        assert_eq!(fm.parameters[0].param_type, "integer");
        assert!(fm.parameters[0].required);
        assert_eq!(fm.sources.len(), 1);
        assert_eq!(fm.sources[0].id.as_deref(), Some("rev-policy"));
        assert_eq!(fm.sources[0].usage_count, Some(5000));
        assert_eq!(trust_tier(&fm), "human-reviewed");
        assert_eq!(
            fm.extensions.get("owner").and_then(|v| v.as_str()),
            Some("team:finance")
        );

        // Round-trip serialization preserves all fields including sources, verified, extensions, and timestamps
        let serialized = OkfDocument::serialize(&fm, body);
        let (fm2_opt, _) = OkfDocument::parse(&serialized).unwrap();
        let fm2 = fm2_opt.unwrap();
        assert_eq!(fm2.sources.len(), 1);
        assert_eq!(
            fm2.sources[0].resource,
            "https://wiki.acme/finance/revenue-recognition"
        );
        assert_eq!(
            fm2.generated.as_ref().map(|g| g.at.as_str()),
            Some("2026-06-28T14:00:00Z")
        );
        assert_eq!(trust_tier(&fm2), "human-reviewed");
        assert_eq!(
            fm2.extensions.get("owner").and_then(|v| v.as_str()),
            Some("team:finance")
        );
    }

    #[test]
    fn test_trust_tiers_and_staleness() {
        let mut fm = OkfFrontmatter::default();
        assert_eq!(trust_tier(&fm), "unverified");

        fm.verified_events = vec![OkfVerification {
            by: "process:finance-nightly".to_string(),
            at: "2026-06-12T08:00:00Z".to_string(),
        }];
        assert_eq!(trust_tier(&fm), "machine-confirmed");

        fm.verified_events.push(OkfVerification {
            by: "human:ahormati".to_string(),
            at: "2026-06-25T09:00:00Z".to_string(),
        });
        assert_eq!(trust_tier(&fm), "human-reviewed");

        // Date-only string without 'T' and offset is ignored per OKF v0.2 §5.5
        fm.stale_after = Some("2026-12-31".to_string());
        let now = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
        assert!(!is_stale(&fm, Some(now)));

        // Full ISO 8601 UTC timestamp
        fm.stale_after = Some("2026-09-23T00:00:00Z".to_string());
        let before = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        assert!(!is_stale(&fm, Some(before)));
        assert!(is_stale(&fm, Some(after)));
    }

    #[test]
    fn test_augmentation_guard_and_bundle_tools() {
        let dir = tempdir().unwrap();
        let bundle = dir.path();

        let mut fm = OkfFrontmatter {
            concept_type: "BigQuery Table".to_string(),
            title: Some("Events Table".to_string()),
            sources: vec![OkfSource {
                id: Some("bq".to_string()),
                resource: "bigquery://proj.dataset.events".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let body = "# Schema\n- `event_date` (STRING)\n- `event_name` (STRING)\n";

        write_concept_doc(bundle, "tables/events_", fm.clone(), body, true).unwrap();

        // Attempting to shrink # Schema fields must be rejected by augmentation guard
        let shrunk_body = "# Schema\n- `event_date` (STRING)\n";
        let err = write_concept_doc(bundle, "tables/events_", fm.clone(), shrunk_body, true);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("event_name"));

        // Attempting to shrink `sources` must be rejected by augmentation guard
        fm.sources.clear();
        let err2 = write_concept_doc(bundle, "tables/events_", fm.clone(), body, true);
        assert!(err2.is_err());
        assert!(err2.unwrap_err().to_string().contains("sources"));
    }

    #[test]
    fn test_attested_computation_and_visualizer() {
        let dir = tempdir().unwrap();
        let bundle = dir.path();

        let comp_fm = OkfFrontmatter {
            concept_type: "Attested Computation".to_string(),
            title: Some("Revenue for fiscal year".to_string()),
            runtime: Some("bigquery".to_string()),
            parameters: vec![OkfParameter {
                name: "year".to_string(),
                param_type: "integer".to_string(),
                required: true,
            }],
            executor: Some(OkfExecutor {
                resource: "references/skills/run-on-bq.md".to_string(),
                receipt: vec![
                    "job_id".to_string(),
                    "executed_sql".to_string(),
                    "result".to_string(),
                ],
            }),
            attester: Some(OkfAttester {
                resource: "references/attesters/sql-equality.py".to_string(),
            }),
            verified_events: vec![OkfVerification {
                by: "human:ahormati".to_string(),
                at: "2026-06-25T09:00:00Z".to_string(),
            }],
            ..Default::default()
        };
        let comp_body = "# Computation\n\n```sql\nSELECT SUM(amount) AS revenue\nFROM finance.recognized_revenue\nWHERE fiscal_year = @year\n```\n";
        write_concept_doc(bundle, "computations/revenue", comp_fm, comp_body, false).unwrap();

        let metric_fm = OkfFrontmatter {
            concept_type: "Metric".to_string(),
            title: Some("Income Statement".to_string()),
            ..Default::default()
        };
        let metric_body = "# Definition\nComputed by [revenue](../computations/revenue.md).\n";
        write_concept_doc(
            bundle,
            "metrics/income-statement",
            metric_fm,
            metric_body,
            false,
        )
        .unwrap();

        // 1. Test deterministic attestation (passing receipt)
        let good_receipt = json!({
            "job_id": "bqjob_123",
            "executed_sql": "select sum(amount) as revenue -- inline comment\nfrom finance.recognized_revenue where fiscal_year = @year",
            "result": [4250000]
        });
        let verdict = attest_concept_doc(
            bundle,
            "computations/revenue",
            &good_receipt,
            Some(&json!(4250000)),
            None,
        )
        .unwrap();
        assert!(verdict.ok, "Expected attestation to pass: {:?}", verdict);

        // 2. Test tampered SQL fails attestation
        let tampered_receipt = json!({
            "job_id": "bqjob_124",
            "executed_sql": "SELECT SUM(amount) AS revenue FROM finance.unrecognized_revenue WHERE fiscal_year = @year",
            "result": [4250000]
        });
        let bad_verdict = attest_concept_doc(
            bundle,
            "computations/revenue",
            &tampered_receipt,
            Some(&json!(4250000)),
            None,
        )
        .unwrap();
        assert!(!bad_verdict.ok);

        // 3. Test recursive bottom-up index generation and HTML visualization
        let indexes = regenerate_indexes(bundle, true).unwrap();
        assert_eq!(indexes.len(), 3); // computations/index.md, metrics/index.md, index.md
        let root_idx = fs::read_to_string(bundle.join("index.md")).unwrap();
        assert!(root_idx.contains("okf_version: \"0.2\""));
        assert!(root_idx.contains("# Subdirectories"));
        assert!(root_idx.contains("computations/index.md"));

        let stats = generate_visualization(bundle, None, Some("Finance OKF")).unwrap();
        assert_eq!(stats.concepts, 2);
        assert_eq!(stats.edges, 1);
        assert!(bundle.join("viz.html").exists());
    }

    #[test]
    fn test_legacy_v01_fallback_and_multi_verified_list() {
        let legacy_doc = r#"---
type: Metric
title: Income statement (fiscal year)
tags: [finance, income-statement]
timestamp: '2026-05-28T22:53:05+00:00'
verified:
  - { by: "process:finance-nightly", at: "2026-06-12T08:00:00Z" }
  - { by: "human:ahormati", at: "2026-06-25T09:00:00Z" }
---

# Definition
Headline income-statement figures.

# Citations
- https://wiki.acme/finance/fpa-handbook
- https://wiki.acme/finance/revenue-recognition
"#;

        let (fm_opt, _) = OkfDocument::parse(legacy_doc).unwrap();
        let fm = fm_opt.unwrap();
        assert_eq!(
            fm.generated.as_ref().map(|g| g.at.as_str()),
            Some("2026-05-28T22:53:05+00:00")
        );
        assert_eq!(fm.sources.len(), 2);
        assert_eq!(
            fm.sources[0].resource,
            "https://wiki.acme/finance/fpa-handbook"
        );
        assert_eq!(normalize_verified(&fm).len(), 2);
        assert_eq!(trust_tier(&fm), "human-reviewed");
    }

    #[test]
    fn test_concept_id_validation_and_web_crawl_guardrails() {
        assert!(parse_concept_id("tables/events_").is_ok());
        assert!(parse_concept_id("computations/rev-2026.v1").is_ok());
        assert!(parse_concept_id("../etc/passwd").is_err());
        assert!(parse_concept_id("-bad/segment").is_err());
        assert!(parse_concept_id("").is_err());

        let mut crawl = OkfWebCrawlState::new(
            &["https://docs.acme.com/guide/start".to_string()],
            &[],
            vec!["/guide/".to_string()],
            vec!["/guide/login".to_string()],
            2,
            1,
        );

        // Seed URL at depth 0 succeeds
        assert_eq!(
            crawl.check_and_mark_url("https://docs.acme.com/guide/start"),
            Ok(0)
        );
        // Duplicate visit rejected
        assert!(crawl
            .check_and_mark_url("https://docs.acme.com/guide/start")
            .is_err());

        // Register child links at depth 1
        crawl.register_discovered_links(
            0,
            &[
                "https://docs.acme.com/guide/schema".to_string(),
                "https://docs.acme.com/guide/login".to_string(),
                "https://evil.example.com/guide/schema".to_string(),
            ],
        );

        // Denied path substring rejected
        assert!(crawl
            .check_and_mark_url("https://docs.acme.com/guide/login")
            .is_err());
        // Disallowed host rejected
        assert!(crawl
            .check_and_mark_url("https://evil.example.com/guide/schema")
            .is_err());
        // Valid depth-1 link succeeds
        assert_eq!(
            crawl.check_and_mark_url("https://docs.acme.com/guide/schema"),
            Ok(1)
        );
        // max_pages (2) reached
        crawl.register_discovered_links(1, &["https://docs.acme.com/guide/extra".to_string()]);
        assert!(crawl
            .check_and_mark_url("https://docs.acme.com/guide/extra")
            .is_err());
    }

    #[test]
    fn test_log_format_and_bundle_conformance() {
        let dir = tempdir().unwrap();
        let bundle = dir.path();

        OkfManager::append_log_entry(
            bundle,
            "human:alice",
            "Creation",
            "tables/users.md",
            "Created Users table specification",
        )
        .unwrap();
        OkfManager::append_log_entry(
            bundle,
            "minicode/v0.4.8",
            "Update",
            "tables/users.md",
            "Added email column to schema",
        )
        .unwrap();

        let log_text = fs::read_to_string(bundle.join("log.md")).unwrap();
        assert!(log_text.starts_with("# Directory Update Log"));
        assert!(log_text.contains("## 20"));
        assert!(log_text.contains("* **Update**: Added email column to schema"));
        assert!(log_text.contains("* **Creation**: Created Users table specification"));

        let fm = OkfFrontmatter {
            concept_type: "Table".to_string(),
            title: Some("Users".to_string()),
            ..Default::default()
        };
        write_concept_doc(
            bundle,
            "tables/users",
            fm,
            "# Overview\nLinks to [missing](/tables/orders.md).\n",
            false,
        )
        .unwrap();

        let report = validate_bundle_conformance(bundle).unwrap();
        assert!(report.conformant);
        assert_eq!(report.total_concepts, 1);
        assert_eq!(report.broken_links.len(), 1);
        assert_eq!(
            report.broken_links[0],
            ("tables/users".to_string(), "tables/orders".to_string())
        );
    }
}
