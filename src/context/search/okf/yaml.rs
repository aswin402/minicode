use super::{
    OkfAttester, OkfExecutor, OkfFrontmatter, OkfGenerated, OkfParameter, OkfSource,
    OkfUsageWindow, OkfVerification,
};
use crate::error::{ContextError, Result};
use indexmap::IndexMap;
use serde_json::{Map, Value};

/// Preferred key order per OKF v0.2 `bundle_tools.py:_PREFERRED_KEY_ORDER`.
pub const PREFERRED_KEY_ORDER: &[&str] = &[
    "type",
    "resource",
    "title",
    "description",
    "tags",
    "status",
    "superseded_by",
    "runtime",
    "computation",
    "parameters",
    "executor",
    "attester",
    "generated",
    "verified",
    "stale_after",
    "sources",
    "usage_window",
];

/// Parses a YAML frontmatter block into a generic ordered map while keeping timestamps as strings.
pub fn parse_yaml_mapping(yaml_str: &str) -> Result<IndexMap<String, Value>> {
    let lines: Vec<&str> = yaml_str.lines().collect();
    let mut map = IndexMap::new();
    let mut idx = 0;

    while idx < lines.len() {
        let raw_line = lines[idx];
        let trimmed = strip_yaml_comment(raw_line).trim();
        if trimmed.is_empty() {
            idx += 1;
            continue;
        }

        let indent = leading_spaces(raw_line);
        if let Some((k, v_rest)) = trimmed.split_once(':') {
            let key = unquote(k.trim());
            let val_str = v_rest.trim();

            if !val_str.is_empty() {
                map.insert(key, parse_inline_value(val_str));
                idx += 1;
            } else {
                idx += 1;
                let (nested_val, next_idx) = parse_indented_block(&lines, idx, indent);
                map.insert(key, nested_val);
                idx = next_idx;
            }
        } else {
            return Err(ContextError::Okf(format!(
                "Invalid YAML in frontmatter at line {}: '{}'",
                idx + 1,
                trimmed
            ))
            .into());
        }
    }

    Ok(map)
}

fn parse_indented_block(lines: &[&str], start_idx: usize, parent_indent: usize) -> (Value, usize) {
    let mut idx = start_idx;
    while idx < lines.len() {
        let trimmed = strip_yaml_comment(lines[idx]).trim();
        if trimmed.is_empty() {
            idx += 1;
        } else {
            break;
        }
    }

    if idx >= lines.len() {
        return (Value::Null, idx);
    }

    let first_indent = leading_spaces(lines[idx]);
    if first_indent <= parent_indent {
        return (Value::Null, idx);
    }

    let first_trimmed = strip_yaml_comment(lines[idx]).trim();
    if first_trimmed.starts_with("- ") || first_trimmed == "-" {
        let mut arr = Vec::new();
        while idx < lines.len() {
            let raw = lines[idx];
            let trimmed = strip_yaml_comment(raw).trim();
            if trimmed.is_empty() {
                idx += 1;
                continue;
            }
            let cur_indent = leading_spaces(raw);
            if cur_indent < first_indent {
                break;
            }
            if cur_indent > first_indent {
                break;
            }
            if !trimmed.starts_with('-') {
                break;
            }

            let after_dash = trimmed[1..].trim();
            if after_dash.is_empty() {
                idx += 1;
                let (item_val, next_i) = parse_indented_block(lines, idx, cur_indent);
                arr.push(item_val);
                idx = next_i;
            } else if after_dash.starts_with('{') && after_dash.ends_with('}') {
                arr.push(parse_inline_mapping(after_dash));
                idx += 1;
            } else if let Some((ik, iv)) = split_mapping_entry(after_dash) {
                let mut item_obj = Map::new();
                let item_key = unquote(ik.trim());
                let iv_trimmed = iv.trim();
                idx += 1;
                if iv_trimmed.is_empty() {
                    let (sub_val, next_i) = parse_indented_block(lines, idx, cur_indent + 1);
                    item_obj.insert(item_key, sub_val);
                    idx = next_i;
                } else {
                    item_obj.insert(item_key, parse_inline_value(iv_trimmed));
                }

                // Read continuation keys of this mapping item at indent > cur_indent
                while idx < lines.len() {
                    let cont_raw = lines[idx];
                    let cont_trimmed = strip_yaml_comment(cont_raw).trim();
                    if cont_trimmed.is_empty() {
                        idx += 1;
                        continue;
                    }
                    let cont_indent = leading_spaces(cont_raw);
                    if cont_indent <= cur_indent {
                        break;
                    }
                    if let Some((ck, cv)) = split_mapping_entry(cont_trimmed) {
                        let ckey = unquote(ck.trim());
                        let cv_trimmed = cv.trim();
                        idx += 1;
                        if cv_trimmed.is_empty() {
                            let (sub_val, next_i) = parse_indented_block(lines, idx, cont_indent);
                            item_obj.insert(ckey, sub_val);
                            idx = next_i;
                        } else {
                            item_obj.insert(ckey, parse_inline_value(cv_trimmed));
                        }
                    } else {
                        break;
                    }
                }
                arr.push(Value::Object(item_obj));
            } else {
                arr.push(parse_inline_value(after_dash));
                idx += 1;
            }
        }
        (Value::Array(arr), idx)
    } else {
        let mut obj = Map::new();
        while idx < lines.len() {
            let raw = lines[idx];
            let trimmed = strip_yaml_comment(raw).trim();
            if trimmed.is_empty() {
                idx += 1;
                continue;
            }
            let cur_indent = leading_spaces(raw);
            if cur_indent < first_indent {
                break;
            }
            if let Some((k, v)) = split_mapping_entry(trimmed) {
                let key = unquote(k.trim());
                let v_trimmed = v.trim();
                idx += 1;
                if v_trimmed.is_empty() {
                    let (sub_val, next_i) = parse_indented_block(lines, idx, cur_indent);
                    obj.insert(key, sub_val);
                    idx = next_i;
                } else {
                    obj.insert(key, parse_inline_value(v_trimmed));
                }
            } else {
                break;
            }
        }
        (Value::Object(obj), idx)
    }
}

fn split_mapping_entry(s: &str) -> Option<(&str, &str)> {
    if s.starts_with('{') || s.starts_with('[') || s.starts_with('"') || s.starts_with('\'') {
        return None;
    }
    let (k, v) = s.split_once(':')?;
    if k.contains(' ') && !k.starts_with('"') && !k.starts_with('\'') {
        return None;
    }
    if v.starts_with("//") {
        // e.g. https://... is a URL, not a mapping entry
        return None;
    }
    Some((k, v))
}

fn parse_inline_value(s: &str) -> Value {
    let trimmed = s.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return parse_inline_mapping(trimmed);
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return parse_inline_array(trimmed);
    }
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return Value::String(unquote(trimmed));
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if trimmed.eq_ignore_ascii_case("null") || trimmed == "~" {
        return Value::Null;
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        if trimmed.contains('.') {
            if let Some(num) = serde_json::Number::from_f64(f) {
                return Value::Number(num);
            }
        }
    }
    Value::String(unquote(trimmed))
}

fn parse_inline_mapping(s: &str) -> Value {
    let inner = s
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    let mut obj = Map::new();
    if inner.is_empty() {
        return Value::Object(obj);
    }
    for part in split_top_level_commas(inner) {
        if let Some((k, v)) = part.split_once(':') {
            obj.insert(unquote(k.trim()), parse_inline_value(v.trim()));
        }
    }
    Value::Object(obj)
}

fn parse_inline_array(s: &str) -> Value {
    let inner = s
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    if inner.is_empty() {
        return Value::Array(Vec::new());
    }
    let items = split_top_level_commas(inner)
        .into_iter()
        .map(|part| parse_inline_value(part.trim()))
        .collect();
    Value::Array(items)
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth_brace = 0usize;
    let mut depth_bracket = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut start = 0usize;
    let bytes = s.as_bytes();

    for i in 0..bytes.len() {
        let ch = bytes[i] as char;
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single && (i == 0 || bytes[i - 1] != b'\\') => in_double = !in_double,
            '{' if !in_single && !in_double => depth_brace += 1,
            '}' if !in_single && !in_double => depth_brace = depth_brace.saturating_sub(1),
            '[' if !in_single && !in_double => depth_bracket += 1,
            ']' if !in_single && !in_double => depth_bracket = depth_bracket.saturating_sub(1),
            ',' if !in_single && !in_double && depth_brace == 0 && depth_bracket == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = &s[start..];
    if !tail.trim().is_empty() {
        parts.push(tail);
    }
    parts
}

fn strip_yaml_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        let ch = bytes[i] as char;
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single && (i == 0 || bytes[i - 1] != b'\\') => in_double = !in_double,
            '#' if !in_single && !in_double => {
                if i == 0 || bytes[i - 1].is_ascii_whitespace() {
                    return &line[..i];
                }
            }
            _ => {}
        }
    }
    line
}

fn leading_spaces(s: &str) -> usize {
    s.chars().take_while(|c| *c == ' ').count()
}

fn unquote(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return trimmed[1..trimmed.len() - 1].replace("\\\"", "\"");
    }
    trimmed.to_string()
}

/// Converts a parsed `IndexMap<String, Value>` (plus optional markdown body for v0.1 `# Citations` fallback)
/// into a typed `OkfFrontmatter`.
pub fn frontmatter_from_yaml_str(yaml_str: &str, body_opt: Option<&str>) -> Result<OkfFrontmatter> {
    let raw_map = parse_yaml_mapping(yaml_str)?;
    let concept_type = raw_map
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ContextError::Okf("Missing required OKF 'type' field in frontmatter".to_string())
        })?;

    let mut fm = OkfFrontmatter {
        concept_type,
        title: raw_map.get("title").and_then(val_to_string),
        description: raw_map.get("description").and_then(val_to_string),
        resource: raw_map.get("resource").and_then(val_to_string),
        tags: parse_string_list(raw_map.get("tags")),
        sources: parse_sources(raw_map.get("sources")),
        generated: parse_generated(raw_map.get("generated"), raw_map.get("timestamp")),
        verified: None,
        verified_events: parse_verified_events(raw_map.get("verified")),
        status: raw_map.get("status").and_then(val_to_string),
        superseded_by: raw_map.get("superseded_by").and_then(val_to_string),
        stale_after: raw_map.get("stale_after").and_then(val_to_string),
        usage_window: parse_usage_window(raw_map.get("usage_window")),
        runtime: raw_map.get("runtime").and_then(val_to_string),
        parameters: parse_parameters(raw_map.get("parameters")),
        computation: raw_map.get("computation").and_then(val_to_string),
        executor: parse_executor(raw_map.get("executor")),
        attester: parse_attester(raw_map.get("attester")),
        extensions: IndexMap::new(),
    };

    fm.verified = fm.verified_events.first().cloned();

    // Legacy OKF v0.1 §13.1 fallback: parse `# Citations` body list if `sources` is absent
    if fm.sources.is_empty() {
        if let Some(body) = body_opt {
            fm.sources = extract_legacy_citations(body);
        }
    }

    // Preserve arbitrary producer extension keys (§4.1)
    for (k, v) in raw_map {
        if !PREFERRED_KEY_ORDER.contains(&k.as_str()) && k != "timestamp" {
            fm.extensions.insert(k, v);
        }
    }

    Ok(fm)
}

/// Serializes `OkfFrontmatter` in canonical `_PREFERRED_KEY_ORDER` followed by extension keys.
pub fn frontmatter_to_yaml_string(fm: &OkfFrontmatter) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!("type: {}\n", format_scalar(&fm.concept_type)));

    if let Some(r) = &fm.resource {
        out.push_str(&format!("resource: {}\n", format_scalar(r)));
    }
    if let Some(t) = &fm.title {
        out.push_str(&format!("title: \"{}\"\n", escape_quotes(t)));
    }
    if let Some(d) = &fm.description {
        out.push_str(&format!("description: \"{}\"\n", escape_quotes(d)));
    }
    if !fm.tags.is_empty() {
        out.push_str(&format!("tags: [{}]\n", fm.tags.join(", ")));
    }
    if let Some(s) = &fm.status {
        out.push_str(&format!("status: {}\n", format_scalar(s)));
    }
    if let Some(sb) = &fm.superseded_by {
        out.push_str(&format!("superseded_by: {}\n", format_scalar(sb)));
    }
    if let Some(rt) = &fm.runtime {
        out.push_str(&format!("runtime: {}\n", format_scalar(rt)));
    }
    if let Some(comp) = &fm.computation {
        out.push_str(&format!("computation: {}\n", format_scalar(comp)));
    }
    if !fm.parameters.is_empty() {
        out.push_str("parameters:\n");
        for p in &fm.parameters {
            out.push_str(&format!(
                "  - {{ name: {}, type: {}, required: {} }}\n",
                format_scalar(&p.name),
                format_scalar(&p.param_type),
                p.required
            ));
        }
    }
    if let Some(exec) = &fm.executor {
        out.push_str("executor:\n");
        out.push_str(&format!("  resource: {}\n", format_scalar(&exec.resource)));
        if !exec.receipt.is_empty() {
            out.push_str(&format!("  receipt: [{}]\n", exec.receipt.join(", ")));
        }
    }
    if let Some(att) = &fm.attester {
        out.push_str("attester:\n");
        out.push_str(&format!("  resource: {}\n", format_scalar(&att.resource)));
    }
    if let Some(g) = &fm.generated {
        out.push_str(&format!(
            "generated: {{ by: \"{}\", at: \"{}\" }}\n",
            escape_quotes(&g.by),
            escape_quotes(&g.at)
        ));
    }

    let events = super::normalize_verified(fm);
    if events.len() == 1 {
        let v = &events[0];
        out.push_str(&format!(
            "verified: {{ by: \"{}\", at: \"{}\" }}\n",
            escape_quotes(&v.by),
            escape_quotes(&v.at)
        ));
    } else if events.len() > 1 {
        out.push_str("verified:\n");
        for v in &events {
            out.push_str(&format!(
                "  - {{ by: \"{}\", at: \"{}\" }}\n",
                escape_quotes(&v.by),
                escape_quotes(&v.at)
            ));
        }
    }

    if let Some(sa) = &fm.stale_after {
        out.push_str(&format!("stale_after: {}\n", format_scalar(sa)));
    }
    if !fm.sources.is_empty() {
        out.push_str("sources:\n");
        for src in &fm.sources {
            if let Some(id) = &src.id {
                out.push_str(&format!("  - id: {}\n", format_scalar(id)));
                out.push_str(&format!("    resource: {}\n", format_scalar(&src.resource)));
            } else {
                out.push_str(&format!("  - resource: {}\n", format_scalar(&src.resource)));
            }
            if let Some(title) = &src.title {
                out.push_str(&format!("    title: \"{}\"\n", escape_quotes(title)));
            }
            if let Some(author) = &src.author {
                out.push_str(&format!("    author: {}\n", format_scalar(author)));
            }
            if let Some(cnt) = src.usage_count {
                out.push_str(&format!("    usage_count: {}\n", cnt));
            }
            if let Some(lm) = &src.last_modified {
                out.push_str(&format!("    last_modified: {}\n", format_scalar(lm)));
            }
        }
    }
    if let Some(uw) = &fm.usage_window {
        out.push_str(&format!(
            "usage_window: {{ from: {}, to: {} }}\n",
            format_scalar(&uw.from),
            format_scalar(&uw.to)
        ));
    }

    for (ext_k, ext_v) in &fm.extensions {
        out.push_str(&format!("{}: {}\n", ext_k, format_value_inline(ext_v)));
    }

    Ok(out)
}

fn val_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn parse_string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(arr)) => arr.iter().filter_map(val_to_string).collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

fn parse_generated(gen_val: Option<&Value>, legacy_ts: Option<&Value>) -> Option<OkfGenerated> {
    if let Some(Value::Object(obj)) = gen_val {
        let by = obj
            .get("by")
            .and_then(val_to_string)
            .unwrap_or_else(|| "unknown".to_string());
        let at = obj.get("at").and_then(val_to_string).unwrap_or_default();
        if !by.is_empty() || !at.is_empty() {
            return Some(OkfGenerated { by, at });
        }
    }
    // OKF v0.2 §13.1 fallback: legacy `timestamp`
    if let Some(ts) = legacy_ts.and_then(val_to_string) {
        return Some(OkfGenerated {
            by: "legacy/v0.1".to_string(),
            at: ts,
        });
    }
    None
}

fn parse_verified_events(v: Option<&Value>) -> Vec<OkfVerification> {
    match v {
        Some(Value::Object(obj)) => {
            let by = obj.get("by").and_then(val_to_string).unwrap_or_default();
            let at = obj.get("at").and_then(val_to_string).unwrap_or_default();
            if !by.is_empty() {
                vec![OkfVerification { by, at }]
            } else {
                Vec::new()
            }
        }
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let by = obj.get("by").and_then(val_to_string)?;
                let at = obj.get("at").and_then(val_to_string).unwrap_or_default();
                Some(OkfVerification { by, at })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_sources(v: Option<&Value>) -> Vec<OkfSource> {
    let parse_one = |obj: &Map<String, Value>| -> Option<OkfSource> {
        let resource = obj.get("resource").and_then(val_to_string)?;
        Some(OkfSource {
            id: obj.get("id").and_then(val_to_string),
            resource,
            title: obj.get("title").and_then(val_to_string),
            author: obj.get("author").and_then(val_to_string),
            usage_count: obj.get("usage_count").and_then(|u| u.as_u64()),
            last_modified: obj.get("last_modified").and_then(val_to_string),
        })
    };

    match v {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| item.as_object().and_then(parse_one))
            .collect(),
        Some(Value::Object(obj)) => parse_one(obj).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn parse_usage_window(v: Option<&Value>) -> Option<OkfUsageWindow> {
    let obj = v?.as_object()?;
    let from = obj.get("from").and_then(val_to_string)?;
    let to = obj.get("to").and_then(val_to_string)?;
    Some(OkfUsageWindow { from, to })
}

fn parse_parameters(v: Option<&Value>) -> Vec<OkfParameter> {
    let Some(Value::Array(arr)) = v else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let name = obj.get("name").and_then(val_to_string)?;
            let param_type = obj
                .get("type")
                .and_then(val_to_string)
                .unwrap_or_else(|| "string".to_string());
            let required = obj
                .get("required")
                .and_then(|b| b.as_bool())
                .unwrap_or(true);
            Some(OkfParameter {
                name,
                param_type,
                required,
            })
        })
        .collect()
}

fn parse_executor(v: Option<&Value>) -> Option<OkfExecutor> {
    let obj = v?.as_object()?;
    let resource = obj.get("resource").and_then(val_to_string)?;
    let receipt = parse_string_list(obj.get("receipt"));
    Some(OkfExecutor { resource, receipt })
}

fn parse_attester(v: Option<&Value>) -> Option<OkfAttester> {
    let obj = v?.as_object()?;
    let resource = obj.get("resource").and_then(val_to_string)?;
    Some(OkfAttester { resource })
}

fn extract_legacy_citations(body: &str) -> Vec<OkfSource> {
    let mut in_citations = false;
    let mut sources = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            in_citations = trimmed.eq_ignore_ascii_case("# Citations");
            continue;
        }
        if in_citations {
            if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                let res = rest.trim();
                if !res.is_empty() {
                    sources.push(OkfSource {
                        id: None,
                        resource: res.to_string(),
                        title: None,
                        author: None,
                        usage_count: None,
                        last_modified: None,
                    });
                }
            }
        }
    }
    sources
}

fn escape_quotes(s: &str) -> String {
    s.replace('"', "\\\"")
}

fn format_scalar(s: &str) -> String {
    if s.is_empty()
        || s.contains(": ")
        || s.starts_with('{')
        || s.starts_with('[')
        || s.contains('#')
    {
        format!("\"{}\"", escape_quotes(s))
    } else {
        s.to_string()
    }
}

fn format_value_inline(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format_scalar(s),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_value_inline).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(obj) => {
            let entries: Vec<String> = obj
                .iter()
                .map(|(k, val)| format!("{}: {}", k, format_value_inline(val)))
                .collect();
            format!("{{ {} }}", entries.join(", "))
        }
    }
}
