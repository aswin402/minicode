use super::paths::read_existing_doc;
use super::{is_stale, trust_tier, OkfFrontmatter, OkfParameter};
use crate::error::{ContextError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

/// SQL keywords uppercased during canonicalization (`bundles/acme_retail/attesters/sql_equality.py`).
const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "GROUP", "BY", "ORDER", "HAVING", "JOIN", "LEFT", "RIGHT", "INNER",
    "OUTER", "ON", "AS", "AND", "OR", "SUM", "COUNT", "AVG", "MIN", "MAX", "CASE", "WHEN", "THEN",
    "ELSE", "END", "WITH", "UNION", "ALL", "DISTINCT", "LIMIT", "OFFSET",
];

/// Deterministic verdict returned by an OKF v0.2 §10 attester.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttestationVerdict {
    pub ok: bool,
    pub reason: Option<String>,
    pub details: Value,
}

/// Canonicalizes SQL by stripping line/block comments outside string literals,
/// collapsing whitespace, and uppercasing standalone SQL keywords.
/// Matches `bundles/acme_retail/attesters/sql_equality.py:_canonicalize` with lexical quote safety.
pub fn canonicalize_sql(sql: &str) -> String {
    let no_comments = strip_sql_comments_preserving_literals(sql);
    let collapsed = no_comments
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    let mut out = String::with_capacity(collapsed.len());
    let chars: Vec<char> = collapsed.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        // Preserve string literals untouched (supporting both \' and SQL '' escapes)
        if ch == '\'' || ch == '"' {
            let quote = ch;
            out.push(ch);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == quote {
                    if i + 1 < chars.len() && chars[i + 1] == quote {
                        out.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                    if i == 0 || chars[i - 1] != '\\' {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            continue;
        }

        // Preserve ::cast, @bind_var, or :bind_var untouched
        if ch == ':' && i + 1 < chars.len() && chars[i + 1] == ':' {
            out.push_str("::");
            i += 2;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }

        if (ch == '@' || ch == ':')
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
        {
            out.push(ch);
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            // Don't uppercase if preceded by '.' (e.g. table.column)
            let preceded_by_dot = start > 0 && chars[start - 1] == '.';
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_ascii_uppercase();
            if !preceded_by_dot && SQL_KEYWORDS.contains(&upper.as_str()) {
                out.push_str(&upper);
            } else {
                out.push_str(&word);
            }
        } else {
            out.push(ch);
            i += 1;
        }
    }

    out
}

/// Single-pass lexical state machine that strips `--` line comments and `/* ... */` block comments
/// while preserving `'...'` and `"..."` string/identifier literals intact.
fn strip_sql_comments_preserving_literals(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        // Quoted literal state: copy verbatim
        if ch == '\'' || ch == '"' {
            let quote = ch;
            out.push(ch);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == quote {
                    if i + 1 < chars.len() && chars[i + 1] == quote {
                        out.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                    if i == 0 || chars[i - 1] != '\\' {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            continue;
        }

        // Block comment state: /* ... */
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            out.push(' ');
            continue;
        }

        // Line comment state: -- ... \n
        if ch == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            out.push(' ');
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

/// Extracts the sanctioned computation from either the `computation:` file path or the inline `# Computation` body section (§10.3).
pub fn extract_computation_text(
    bundle_root: &Path,
    fm: &OkfFrontmatter,
    body: &str,
) -> Result<String> {
    if let Some(comp_path_str) = &fm.computation {
        let rel = comp_path_str.trim().trim_start_matches('/');
        let full_path = bundle_root.join(rel);
        let content = fs::read_to_string(&full_path).map_err(|e| {
            ContextError::Okf(format!(
                "Failed to read computation file '{}': {}",
                full_path.display(),
                e
            ))
        })?;
        return Ok(content.trim().to_string());
    }

    // Extract from inline `# Computation` section
    let mut in_comp_section = false;
    let mut section_lines = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            if in_comp_section {
                break;
            }
            if trimmed.eq_ignore_ascii_case("# Computation") {
                in_comp_section = true;
            }
            continue;
        }
        if in_comp_section {
            section_lines.push(line);
        }
    }

    if section_lines.is_empty() {
        return Err(ContextError::Okf(
            "Attested Computation is missing both `computation` file path and inline `# Computation` section".to_string(),
        )
        .into());
    }

    // Check for fenced code block ```...``` first
    let mut in_fence = false;
    let mut fence_lines = Vec::new();
    for line in &section_lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_fence {
                break;
            }
            in_fence = true;
            continue;
        }
        if in_fence {
            fence_lines.push(*line);
        }
    }
    if !fence_lines.is_empty() {
        return Ok(fence_lines.join("\n").trim().to_string());
    }

    // Check for 4-space or tab-indented code block (as used in SPEC.md §10.2)
    let mut indented_lines = Vec::new();
    for line in &section_lines {
        if line.starts_with("    ") || line.starts_with('\t') {
            indented_lines.push(line.trim_start());
        } else if !indented_lines.is_empty() && line.trim().is_empty() {
            indented_lines.push("");
        } else if !indented_lines.is_empty() {
            break;
        }
    }
    if !indented_lines.is_empty() {
        return Ok(indented_lines.join("\n").trim().to_string());
    }

    Err(ContextError::Okf(
        "No fenced or 4-space indented code block found under `# Computation`".to_string(),
    )
    .into())
}

/// Validates and binds typed parameters into a computation template per `runtime` (§10.2 & §10.3).
pub fn bind_parameters(
    computation: &str,
    runtime: &str,
    declared_params: &[OkfParameter],
    param_values: &Map<String, Value>,
) -> Result<String> {
    for param in declared_params {
        match param_values.get(&param.name) {
            Some(val) if !val.is_null() => {
                validate_param_type(&param.name, &param.param_type, val)?;
            }
            _ => {
                if param.required {
                    return Err(ContextError::Okf(format!(
                        "Missing required parameter '{}' (type: {})",
                        param.name, param.param_type
                    ))
                    .into());
                }
            }
        }
    }

    // Reject undeclared parameters
    for provided_key in param_values.keys() {
        if !declared_params.iter().any(|p| &p.name == provided_key) {
            return Err(ContextError::Okf(format!(
                "Undeclared parameter '{}' supplied to Attested Computation",
                provided_key
            ))
            .into());
        }
    }

    let rt_lower = runtime.to_ascii_lowercase();
    let mut bindings = std::collections::HashMap::with_capacity(declared_params.len());
    for param in declared_params {
        if let Some(val) = param_values.get(&param.name) {
            let formatted = format_param_literal(&param.param_type, val, &rt_lower);
            bindings.insert(param.name.as_str(), formatted);
        }
    }

    Ok(bind_sql_template_tokens(computation, &bindings))
}

/// Single-pass lexical state machine that binds `@param`, `:param`, and `{{ var('param') }}`
/// strictly on exact token boundaries outside SQL string literals, comments, and `::type` casts.
fn bind_sql_template_tokens(
    computation: &str,
    bindings: &std::collections::HashMap<&str, String>,
) -> String {
    if bindings.is_empty() {
        return computation.to_string();
    }

    let mut out = String::with_capacity(computation.len() + 32);
    let chars: Vec<char> = computation.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // 1. String/identifier literals ('...' or "..."): copy verbatim without binding
        if ch == '\'' || ch == '"' {
            let quote = ch;
            out.push(ch);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == quote {
                    if i + 1 < chars.len() && chars[i + 1] == quote {
                        out.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                    if i == 0 || chars[i - 1] != '\\' {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            continue;
        }

        // 2. Block comments /* ... */: copy verbatim without binding
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            out.push_str("/*");
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                out.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() {
                out.push_str("*/");
                i += 2;
            }
            continue;
        }

        // 3. Line comments -- ... \n: copy verbatim without binding
        if ch == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            while i < chars.len() && chars[i] != '\n' {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // 4. dbt / Jinja {{ var('param') }} template expression
        if ch == '{' && i + 1 < chars.len() && chars[i + 1] == '{' {
            let start = i;
            let mut close_idx = None;
            let mut j = i + 2;
            while j + 1 < chars.len() {
                if chars[j] == '}' && chars[j + 1] == '}' {
                    close_idx = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(end_braces) = close_idx {
                let inner: String = chars[i + 2..end_braces].iter().collect();
                if let Some(var_name) = parse_jinja_var_call(&inner) {
                    if let Some(bound_val) = bindings.get(var_name) {
                        out.push_str(bound_val);
                        i = end_braces + 2;
                        continue;
                    }
                }
                let raw_expr: String = chars[start..end_braces + 2].iter().collect();
                out.push_str(&raw_expr);
                i = end_braces + 2;
                continue;
            }
        }

        // 5. Postgres ::type cast: emit `::` and its type identifier verbatim
        if ch == ':' && i + 1 < chars.len() && chars[i + 1] == ':' {
            out.push_str("::");
            i += 2;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // 6. Symbolic @param or :param bind variable on exact identifier boundaries
        let prev_is_ident = i > 0
            && (chars[i - 1].is_ascii_alphanumeric() || matches!(chars[i - 1], '_' | '@' | ':'));
        if (ch == '@' || ch == ':')
            && !prev_is_ident
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
        {
            let ident_start = i + 1;
            let mut ident_end = ident_start;
            while ident_end < chars.len()
                && (chars[ident_end].is_ascii_alphanumeric() || chars[ident_end] == '_')
            {
                ident_end += 1;
            }
            let ident: String = chars[ident_start..ident_end].iter().collect();
            if let Some(bound_val) = bindings.get(ident.as_str()) {
                out.push_str(bound_val);
            } else {
                out.push(ch);
                out.push_str(&ident);
            }
            i = ident_end;
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

/// Parses a Jinja `var('name')` or `var("name")` call inside `{{ ... }}` with arbitrary whitespace.
fn parse_jinja_var_call(inner: &str) -> Option<&str> {
    let trimmed = inner.trim();
    let rest = trimmed.strip_prefix("var")?.trim_start();
    let rest = rest.strip_prefix('(')?.trim_start();
    let rest = rest.strip_suffix(')')?.trim_end();
    if ((rest.starts_with('\'') && rest.ends_with('\''))
        || (rest.starts_with('"') && rest.ends_with('"')))
        && rest.len() >= 2
    {
        return Some(&rest[1..rest.len() - 1]);
    }
    None
}

fn validate_param_type(name: &str, expected_type: &str, val: &Value) -> Result<()> {
    let ok = match expected_type.to_ascii_lowercase().as_str() {
        "integer" | "int" | "int64" => val.is_i64() || val.is_u64(),
        "number" | "float" | "float64" | "numeric" | "decimal" => val.is_number(),
        "boolean" | "bool" => val.is_boolean(),
        "string" | "str" | "date" | "timestamp" => val.is_string(),
        _ => true,
    };
    if !ok {
        return Err(ContextError::Okf(format!(
            "Parameter '{}' expected type '{}', got {}",
            name, expected_type, val
        ))
        .into());
    }
    Ok(())
}

fn format_param_literal(param_type: &str, val: &Value, _runtime: &str) -> String {
    match param_type.to_ascii_lowercase().as_str() {
        "integer" | "int" | "int64" | "number" | "float" | "float64" | "boolean" | "bool" => {
            val.to_string()
        }
        _ => {
            let s = val.as_str().unwrap_or_default().replace('\'', "''");
            format!("'{}'", s)
        }
    }
}

/// Deterministic attester verifying receipt provenance and result fidelity (`bundles/acme_retail/attesters/sql_equality.py:attest`).
pub fn attest_computation(
    sanctioned_sql: &str,
    receipt: &Value,
    claimed_value: Option<&Value>,
) -> AttestationVerdict {
    let Some(receipt_obj) = receipt.as_object() else {
        return AttestationVerdict {
            ok: false,
            reason: Some("receipt must be a JSON object".to_string()),
            details: json!({}),
        };
    };

    let executed_opt = receipt_obj
        .get("executed_sql")
        .or_else(|| receipt_obj.get("compiled_sql"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty());

    let Some(executed) = executed_opt else {
        let mut keys: Vec<String> = receipt_obj.keys().cloned().collect();
        keys.sort();
        return AttestationVerdict {
            ok: false,
            reason: Some("receipt is missing executed_sql".to_string()),
            details: json!({ "receipt_keys": keys }),
        };
    };

    let can_sanctioned = canonicalize_sql(sanctioned_sql);
    let can_executed = canonicalize_sql(executed);

    if can_sanctioned != can_executed {
        return AttestationVerdict {
            ok: false,
            reason: Some("executed SQL does not match sanctioned computation".to_string()),
            details: json!({
                "sanctioned_canonical": can_sanctioned,
                "executed_canonical": can_executed,
            }),
        };
    }

    let Some(result_val) = receipt_obj.get("result") else {
        return AttestationVerdict {
            ok: false,
            reason: Some("receipt is missing result".to_string()),
            details: json!({}),
        };
    };
    if result_val.is_null() {
        return AttestationVerdict {
            ok: false,
            reason: Some("receipt is missing result".to_string()),
            details: json!({}),
        };
    }

    if let Some(claimed) = claimed_value {
        let first_cell = match result_val {
            Value::Array(arr) => arr.first().unwrap_or(&Value::Null),
            other => other,
        };
        if !values_equivalent(first_cell, claimed) {
            return AttestationVerdict {
                ok: false,
                reason: Some("claimed value does not match receipt result".to_string()),
                details: json!({
                    "claimed": claimed,
                    "receipt_first_cell": first_cell,
                }),
            };
        }
    }

    AttestationVerdict {
        ok: true,
        reason: None,
        details: json!({
            "job_id": receipt_obj.get("job_id").or_else(|| receipt_obj.get("run_id")).cloned().unwrap_or(Value::Null),
        }),
    }
}

fn values_equivalent(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    match (a.as_f64(), b.as_f64()) {
        (Some(fa), Some(fb)) => (fa - fb).abs() < 1e-9,
        _ => false,
    }
}

/// Resolves the enclosing workspace root for an OKF bundle directory by walking upward for
/// repository markers (`.git`, `.minicode`, `Cargo.toml`, `package.json`), falling back to `bundle_root`.
pub(crate) fn find_workspace_root_for_bundle(bundle_root: &Path) -> std::path::PathBuf {
    let mut cur = Some(bundle_root);
    while let Some(dir) = cur {
        if dir.join(".git").exists()
            || dir.join(".minicode").is_dir()
            || dir.join("Cargo.toml").is_file()
            || dir.join("package.json").is_file()
        {
            return dir.to_path_buf();
        }
        cur = dir.parent();
    }
    bundle_root.to_path_buf()
}

/// End-to-end OKF v0.2 §10.5 consumer attestation gate for an `Attested Computation` (SQL)
/// or AST-attested code concept (`extensions.ast_hash` + `code_ref` / `resource#symbol`) in a bundle.
pub fn attest_concept_doc(
    bundle_root: &Path,
    concept_id: &str,
    receipt: &Value,
    claimed_value: Option<&Value>,
    param_values: Option<&Map<String, Value>>,
) -> Result<AttestationVerdict> {
    let Some((fm, body)) = read_existing_doc(bundle_root, concept_id)? else {
        return Err(
            ContextError::Okf(format!("Concept '{}' not found in bundle", concept_id)).into(),
        );
    };

    // Support direct Tree-sitter AST code symbol attestation when `ast_hash` is declared
    let ws_root = find_workspace_root_for_bundle(bundle_root);
    let receipt_has_sql = receipt
        .as_object()
        .is_some_and(|o| o.contains_key("executed_sql") || o.contains_key("sql"));
    if fm.extensions.contains_key("ast_hash") && !receipt_has_sql {
        if let Some(mut code_verdict) = attest_code_symbol(&ws_root, &fm) {
            let conf = super::compute_knowledge_confidence(&fm, Some(&ws_root), None);
            if let Some(details_obj) = code_verdict.details.as_object_mut() {
                details_obj.insert("concept_id".to_string(), json!(concept_id));
                details_obj.insert("trust_tier".to_string(), json!(conf.trust_tier));
                details_obj.insert("stale".to_string(), json!(conf.stale));
                details_obj.insert("confidence_multiplier".to_string(), json!(conf.multiplier));
                if !conf.drifted_sources.is_empty() {
                    details_obj.insert("drifted_sources".to_string(), json!(conf.drifted_sources));
                }
            }
            return Ok(code_verdict);
        }
    }

    if fm.concept_type != "Attested Computation" {
        return Err(ContextError::Okf(format!(
            "Concept '{}' has type '{}', expected 'Attested Computation'",
            concept_id, fm.concept_type
        ))
        .into());
    }

    fm.validate()?;
    let runtime = fm.runtime.as_deref().unwrap_or("sql");

    // Check required receipt fields declared by `executor.receipt`
    if let Some(exec) = &fm.executor {
        if let Some(receipt_obj) = receipt.as_object() {
            for req_field in &exec.receipt {
                if !receipt_obj.contains_key(req_field) {
                    return Ok(AttestationVerdict {
                        ok: false,
                        reason: Some(format!(
                            "receipt is missing required executor field '{}'",
                            req_field
                        )),
                        details: json!({
                            "concept_id": concept_id,
                            "missing_receipt_field": req_field,
                        }),
                    });
                }
            }
        }
    }

    let raw_computation = extract_computation_text(bundle_root, &fm, &body)?;

    // First try direct (unbound/symbolic @param) match; if `param_values` are provided, also allow bound match
    let mut verdict = attest_computation(&raw_computation, receipt, claimed_value);
    if !verdict.ok {
        if let Some(params) = param_values {
            let bound = bind_parameters(&raw_computation, runtime, &fm.parameters, params)?;
            let bound_verdict = attest_computation(&bound, receipt, claimed_value);
            if bound_verdict.ok
                || bound_verdict.reason.as_deref()
                    != Some("executed SQL does not match sanctioned computation")
            {
                verdict = bound_verdict;
            }
        }
    }

    let stale = is_stale(&fm, None);
    let tier = trust_tier(&fm);
    if let Some(details_obj) = verdict.details.as_object_mut() {
        details_obj.insert("concept_id".to_string(), json!(concept_id));
        details_obj.insert("runtime".to_string(), json!(runtime));
        details_obj.insert("trust_tier".to_string(), json!(tier));
        details_obj.insert("stale".to_string(), json!(stale));
        if let Some(sa) = &fm.stale_after {
            details_obj.insert("stale_after".to_string(), json!(sa));
        }
    }

    Ok(verdict)
}

/// Computes a formatting- and comment-invariant Tree-sitter AST fingerprint (`"ast_fnv1a64:<hex>"`)
/// for a named code symbol (`function`, `struct`, `class`, `trait`, `interface`, `enum`, etc.)
/// located inside `workspace_root/file_path`.
///
/// Reuses `RepoMapExtractor` for cross-language symbol span extraction and `SyntaxGuard` Tree-sitter
/// grammars to traverse semantic leaf atoms (ignoring whitespace and comment nodes).
pub fn compute_ast_symbol_hash(
    workspace_root: &Path,
    file_path: &str,
    symbol_name: &str,
) -> Result<String> {
    let rel = super::resolve_repo_source_path(file_path).ok_or_else(|| {
        ContextError::Okf(format!("Invalid repo-relative code path '{}'", file_path))
    })?;
    let abs = workspace_root.join(&rel);
    let source = fs::read_to_string(&abs).map_err(|e| {
        ContextError::Okf(format!(
            "Failed to read source file '{}' for AST attestation: {}",
            rel, e
        ))
    })?;
    let ext = abs.extension().and_then(|e| e.to_str()).unwrap_or("");
    let extractor = crate::context::repomap::RepoMapExtractor::new();
    let symbols = extractor.extract_symbols_from_str(ext, &source)?;

    let sym = symbols
        .iter()
        .find(|s| s.name == symbol_name && s.kind != "import")
        .ok_or_else(|| {
            ContextError::Okf(format!(
                "Symbol '{}' not found in '{}' for AST attestation",
                symbol_name, rel
            ))
        })?;

    let lines: Vec<&str> = source.lines().collect();
    let start_idx = sym.line_number.saturating_sub(1).min(lines.len());
    let end_idx = sym.end_line.min(lines.len()).max(start_idx);
    let sym_slice = lines[start_idx..end_idx].join("\n");

    let mut hash: u64 = crate::constants::FNV_OFFSET_BASIS;
    let feed = |h: &mut u64, bytes: &[u8]| {
        for &b in bytes {
            *h ^= u64::from(b);
            *h = h.wrapping_mul(crate::constants::FNV_PRIME);
        }
    };
    feed(&mut hash, sym.kind.as_bytes());
    feed(&mut hash, &[0x1F]);
    feed(&mut hash, sym.name.as_bytes());
    feed(&mut hash, &[0x1F]);

    if let Some(lang) = crate::context::syntax_guard::SyntaxGuard::language_for_extension(ext) {
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&lang).is_ok() {
            if let Some(tree) = parser.parse(&sym_slice, None) {
                let mut leaves: Vec<(&str, &str)> = Vec::new();
                let mut stack = vec![tree.root_node()];
                while let Some(node) = stack.pop() {
                    let kind = node.kind();
                    if kind.contains("comment") {
                        continue;
                    }
                    if node.child_count() == 0 {
                        if let Ok(text) = node.utf8_text(sym_slice.as_bytes()) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                leaves.push((kind, trimmed));
                            }
                        }
                    } else {
                        for idx in (0..node.child_count()).rev() {
                            if let Some(child) = node.child(idx) {
                                stack.push(child);
                            }
                        }
                    }
                }
                // Strip formatter-inserted trailing commas (rustfmt / prettier / black) immediately
                // preceding closing delimiters while keeping all operators, identifiers, and literals.
                for (i, &(kind, text)) in leaves.iter().enumerate() {
                    let is_trailing_comma = text == ","
                        && leaves
                            .get(i + 1)
                            .is_some_and(|&(_, next)| matches!(next, ")" | "]" | "}" | ">"));
                    if is_trailing_comma {
                        continue;
                    }
                    feed(&mut hash, kind.as_bytes());
                    feed(&mut hash, &[0x1E]);
                    feed(&mut hash, text.as_bytes());
                    feed(&mut hash, &[0x1D]);
                }
                return Ok(format!("ast_fnv1a64:{:016x}", hash));
            }
        }
    }

    // Fallback for extensions without a registered Tree-sitter grammar: whitespace-normalized tokens
    for tok in sym_slice.split_whitespace() {
        feed(&mut hash, tok.as_bytes());
        feed(&mut hash, &[0x1D]);
    }
    Ok(format!("ast_fnv1a64:{:016x}", hash))
}

/// Extracts `(file_path, symbol_name)` from OKF frontmatter `extensions.code_ref` or `resource` (`path#symbol`).
fn extract_code_ref(fm: &OkfFrontmatter) -> Option<(String, String)> {
    if let Some(val) = fm.extensions.get("code_ref") {
        if let Some(s) = val.as_str() {
            if let Some((path, sym)) = s.split_once('#') {
                let sym = sym.trim();
                if !sym.is_empty() {
                    return Some((path.trim().to_string(), sym.to_string()));
                }
            }
        } else if let Some(obj) = val.as_object() {
            let path = obj.get("file").and_then(|v| v.as_str())?.trim();
            let sym = obj.get("symbol").and_then(|v| v.as_str())?.trim();
            if !path.is_empty() && !sym.is_empty() {
                return Some((path.to_string(), sym.to_string()));
            }
        }
    }
    if let Some(ref res) = fm.resource {
        if let Some((path, sym)) = res.split_once('#') {
            let sym = sym.trim();
            if !sym.is_empty() && super::resolve_repo_source_path(path).is_some() {
                return Some((path.trim().to_string(), sym.to_string()));
            }
        }
    }
    None
}

/// Verifies an OKF document's `ast_hash` extension against the live Tree-sitter AST of the
/// referenced code symbol (`extensions.code_ref` or `resource: "path/to/file.rs#symbol"`).
///
/// Returns `None` if the document does not declare a code symbol attestation contract.
pub fn attest_code_symbol(
    workspace_root: &Path,
    fm: &OkfFrontmatter,
) -> Option<AttestationVerdict> {
    let expected_hash = fm
        .extensions
        .get("ast_hash")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let (file_path, symbol_name) = extract_code_ref(fm)?;

    match compute_ast_symbol_hash(workspace_root, &file_path, &symbol_name) {
        Ok(actual_hash) => {
            let ok = actual_hash == expected_hash;
            Some(AttestationVerdict {
                ok,
                reason: (!ok).then(|| {
                    format!(
                        "AST fingerprint mismatch for '{}#{}' (expected {}, found {})",
                        file_path, symbol_name, expected_hash, actual_hash
                    )
                }),
                details: json!({
                    "file": file_path,
                    "symbol": symbol_name,
                    "expected_ast_hash": expected_hash,
                    "actual_ast_hash": actual_hash,
                }),
            })
        }
        Err(e) => Some(AttestationVerdict {
            ok: false,
            reason: Some(e.to_string()),
            details: json!({
                "file": file_path,
                "symbol": symbol_name,
                "expected_ast_hash": expected_hash,
            }),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonicalize_sql_preserves_comments_inside_literals() {
        let sql = "select 'hello -- world', \"col /* not comment */\" -- real comment\nfrom orders /* block */ where status = 'ok'";
        let can = canonicalize_sql(sql);
        assert_eq!(
            can,
            "SELECT 'hello -- world', \"col /* not comment */\" FROM orders WHERE status = 'ok'"
        );
    }

    #[test]
    fn test_bind_parameters_lexical_boundary_and_cast_safety() {
        let declared = vec![
            OkfParameter {
                name: "id".to_string(),
                param_type: "integer".to_string(),
                required: true,
            },
            OkfParameter {
                name: "id_list".to_string(),
                param_type: "string".to_string(),
                required: true,
            },
            OkfParameter {
                name: "date".to_string(),
                param_type: "date".to_string(),
                required: true,
            },
        ];
        let mut vals = Map::new();
        vals.insert("id".to_string(), json!(42));
        vals.insert("id_list".to_string(), json!("a,b"));
        vals.insert("date".to_string(), json!("2026-10-31"));

        let template = "SELECT created_at::date, 'email@id.com' FROM t WHERE id = @id AND tag = @id_list AND d = :date AND r = {{ var( 'id_list' ) }}";
        let bound = bind_parameters(template, "postgres", &declared, &vals).unwrap();
        assert_eq!(
            bound,
            "SELECT created_at::date, 'email@id.com' FROM t WHERE id = 42 AND tag = 'a,b' AND d = '2026-10-31' AND r = 'a,b'"
        );
    }

    #[test]
    fn test_ast_symbol_hash_comment_whitespace_invariance_and_code_attestation() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        fs::create_dir_all(ws.join("src")).unwrap();

        let original =
            "pub fn compute_fee(amount: u64) -> u64 {\n    // base fee\n    amount * 2 + 5\n}\n";
        fs::write(ws.join("src/billing.rs"), original).unwrap();
        let h1 = compute_ast_symbol_hash(ws, "src/billing.rs", "compute_fee").unwrap();
        assert!(h1.starts_with("ast_fnv1a64:"));

        // Reformatting + changing comments must preserve the exact semantic AST hash
        let reformatted = "pub fn compute_fee(\n    amount: u64,\n) -> u64 {\n    /* updated comment */\n    amount*2+5\n}\n";
        fs::write(ws.join("src/billing.rs"), reformatted).unwrap();
        let h2 = compute_ast_symbol_hash(ws, "src/billing.rs", "compute_fee").unwrap();
        assert_eq!(
            h1, h2,
            "AST symbol hash must be invariant to formatting and comments"
        );

        let mut fm = OkfFrontmatter::default();
        fm.extensions
            .insert("code_ref".to_string(), json!("src/billing.rs#compute_fee"));
        fm.extensions.insert("ast_hash".to_string(), json!(h1));

        let pass = attest_code_symbol(ws, &fm).expect("verdict expected");
        assert!(pass.ok, "Expected matching AST attestation: {:?}", pass);

        // Changing arithmetic logic must fail AST attestation immediately
        let mutated = "pub fn compute_fee(amount: u64) -> u64 {\n    amount * 3 + 5\n}\n";
        fs::write(ws.join("src/billing.rs"), mutated).unwrap();
        let fail = attest_code_symbol(ws, &fm).expect("verdict expected");
        assert!(!fail.ok, "Mutated code logic must fail AST attestation");
    }
}
