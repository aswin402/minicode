use super::{OkfDocument, OkfFrontmatter, OkfGenerated};
use crate::error::{ContextError, Result};
use chrono::Utc;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Outcome of writing an OKF concept document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteConceptOutcome {
    pub concept_id: String,
    pub relative_path: String,
    pub bytes: usize,
}

/// Validates a single concept ID segment per OKF `bundle/paths.py:_SEGMENT_RE` (`[A-Za-z0-9_][A-Za-z0-9_.\-]*`).
pub fn validate_segment(seg: &str) -> Result<()> {
    if seg.is_empty() || seg == "." || seg == ".." {
        return Err(ContextError::Okf(format!("Invalid concept id segment: {:?}", seg)).into());
    }
    let mut chars = seg.chars();
    let first = chars.next().unwrap_or(' ');
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return Err(ContextError::Okf(format!("Invalid concept id segment: {:?}", seg)).into());
    }
    for ch in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-') {
            return Err(ContextError::Okf(format!("Invalid concept id segment: {:?}", seg)).into());
        }
    }
    Ok(())
}

/// Parses a slash-delimited concept ID (e.g., `"tables/events_"` or `"computations/revenue"`) into validated segments.
pub fn parse_concept_id(s: &str) -> Result<Vec<String>> {
    let trimmed = s.trim().trim_end_matches(".md");
    let parts: Vec<String> = trimmed
        .split('/')
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect();
    if parts.is_empty() {
        return Err(ContextError::Okf(format!("Empty concept id: {:?}", s)).into());
    }
    for p in &parts {
        validate_segment(p)?;
    }
    Ok(parts)
}

/// Maps a validated concept ID slice to its `.md` file path under `bundle_root`.
pub fn concept_id_to_path<S: AsRef<str>>(bundle_root: &Path, concept_id: &[S]) -> Result<PathBuf> {
    if concept_id.is_empty() {
        return Err(
            ContextError::Okf("concept_id must have at least one segment".to_string()).into(),
        );
    }
    for seg in concept_id {
        validate_segment(seg.as_ref())?;
    }
    let mut out = bundle_root.to_path_buf();
    for seg in &concept_id[..concept_id.len() - 1] {
        out.push(seg.as_ref());
    }
    let last = concept_id[concept_id.len() - 1].as_ref();
    out.push(format!("{}.md", last));
    Ok(out)
}

/// Maps a `.md` file path inside `bundle_root` back to its concept ID segments.
pub fn path_to_concept_id(bundle_root: &Path, path: &Path) -> Result<Vec<String>> {
    let rel = path
        .strip_prefix(bundle_root)
        .map_err(|e| ContextError::Okf(format!("Path not inside bundle_root: {}", e)))?;
    let without_ext = rel.with_extension("");
    let mut segments = Vec::new();
    for comp in without_ext.components() {
        let s = comp.as_os_str().to_string_lossy().to_string();
        validate_segment(&s)?;
        segments.push(s);
    }
    if segments.is_empty() {
        return Err(ContextError::Okf("Resolved empty concept_id from path".to_string()).into());
    }
    Ok(segments)
}

/// Extracts non-blank lines under a top-level `# <heading>` section.
pub fn section_content_lines<'a>(body: &'a str, heading: &str) -> Vec<&'a str> {
    let mut in_section = false;
    let mut out = Vec::new();
    for line in body.lines() {
        let stripped = line.trim();
        if stripped.starts_with("# ") {
            in_section = stripped.eq_ignore_ascii_case(heading);
            continue;
        }
        if in_section && !stripped.is_empty() {
            out.push(line);
        }
    }
    out
}

/// Extracts backtick-delimited field names (`field_name`) from the `# Schema` section of a document body.
/// Matches `bundle_tools.py:_schema_field_names`.
pub fn schema_field_names(body: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in section_content_lines(body, "# Schema") {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'`' {
                if let Some(end_offset) = line[i + 1..].find('`') {
                    let token = &line[i + 1..i + 1 + end_offset];
                    if is_valid_schema_field_ident(token) {
                        names.insert(token.to_string());
                    }
                    i += end_offset + 2;
                    continue;
                }
            }
            i += 1;
        }
    }
    names
}

fn is_valid_schema_field_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap_or(' ');
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Reads an existing OKF concept document by `concept_id` if present on disk (`bundle_tools.py:read_existing_doc`).
pub fn read_existing_doc(
    bundle_root: &Path,
    concept_id: &str,
) -> Result<Option<(OkfFrontmatter, String)>> {
    let cid = parse_concept_id(concept_id)?;
    let path = concept_id_to_path(bundle_root, &cid)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| ContextError::Okf(format!("Failed to read {}: {}", path.display(), e)))?;
    let (fm_opt, body) = OkfDocument::parse(&raw)?;
    let fm = fm_opt.unwrap_or_default();
    Ok(Some((fm, body.to_string())))
}

/// Writes or updates an OKF concept document (`bundle_tools.py:write_concept_doc`).
///
/// - Automatically fills in `generated: { by, at }` if missing or incomplete.
/// - Validates required frontmatter keys (`type`, plus `runtime` for `Attested Computation`).
/// - Enforces the **Augmentation Guard** when `enforce_augmentation_guard` is true and the concept
///   already exists on disk: refuses writes that drop existing `# Schema` field names or shrink `sources`.
pub fn write_concept_doc(
    bundle_root: &Path,
    concept_id: &str,
    mut frontmatter: OkfFrontmatter,
    body: &str,
    enforce_augmentation_guard: bool,
) -> Result<WriteConceptOutcome> {
    let cid = parse_concept_id(concept_id)?;
    let path = concept_id_to_path(bundle_root, &cid)?;

    // Auto-fill `generated: { by, at }`
    let now_iso = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let default_actor = format!("minicode/v{}", env!("CARGO_PKG_VERSION"));
    match &mut frontmatter.generated {
        Some(g) => {
            if g.by.trim().is_empty() {
                g.by = default_actor;
            }
            if g.at.trim().is_empty() {
                g.at = now_iso;
            }
        }
        None => {
            frontmatter.generated = Some(OkfGenerated {
                by: default_actor,
                at: now_iso,
            });
        }
    }

    // Validate required keys
    frontmatter.validate()?;

    // Augmentation guard: refuse writes that shrink an existing doc's `# Schema` field set or `sources` list
    if enforce_augmentation_guard && path.exists() {
        if let Ok(existing_raw) = fs::read_to_string(&path) {
            if let Ok((Some(existing_fm), existing_body)) = OkfDocument::parse(&existing_raw) {
                let old_fields = schema_field_names(existing_body);
                if !old_fields.is_empty() {
                    let new_fields = schema_field_names(body);
                    let missing: Vec<&String> = old_fields.difference(&new_fields).collect();
                    if !missing.is_empty() {
                        let shown: Vec<String> = missing
                            .iter()
                            .take(10)
                            .map(|m| format!("`{}`", m))
                            .collect();
                        let truncated = if missing.len() > 10 {
                            " (and more)"
                        } else {
                            ""
                        };
                        return Err(ContextError::Okf(format!(
                            "Refusing to write `{}`: the existing # Schema section lists {} field(s), \
                             but your new # Schema is missing {} of them: {}{}. \
                             Augment by adding to the existing schema, not replacing it.",
                            concept_id,
                            old_fields.len(),
                            missing.len(),
                            shown.join(", "),
                            truncated
                        ))
                        .into());
                    }
                }

                let old_sources = existing_fm.sources.len();
                let new_sources = frontmatter.sources.len();
                if old_sources > 0 && new_sources < old_sources {
                    return Err(ContextError::Okf(format!(
                        "Refusing to write `{}`: the existing `sources` frontmatter had {} entr(y/ies), \
                         but your new `sources` has only {}. Merge your new source into the existing \
                         list rather than replacing it.",
                        concept_id, old_sources, new_sources
                    ))
                    .into());
                }
            }
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            ContextError::Okf(format!(
                "Failed to create parent directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let serialized = OkfDocument::serialize(&frontmatter, body);
    fs::write(&path, &serialized)
        .map_err(|e| ContextError::Okf(format!("Failed to write {}: {}", path.display(), e)))?;

    let rel_path = path
        .strip_prefix(bundle_root)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/");

    Ok(WriteConceptOutcome {
        concept_id: cid.join("/"),
        relative_path: rel_path,
        bytes: serialized.len(),
    })
}
