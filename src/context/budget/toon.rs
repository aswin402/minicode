//! Native Rust implementation of TOON v3 (Token-Oriented Object Notation, `toon-format/toon`).
//!
//! Provides a 100% lossless, zero-dependency recursive encoder and decoder over `serde_json::Value`:
//! - YAML-like indentation hierarchy for nested objects with safe single-key path folding (`a.b.c:`)
//! - CSV-like tabular arrays (`items[N]{col1,col2}:`) for uniform/semi-uniform arrays of flat objects
//! - Inline primitive arrays (`tags[3]: a,b,c`) and `- ` list items for mixed/nested arrays
//! - Algorithmic per-table delimiter selection (`,` vs `|` vs `\t`) to minimize quoting overhead
//! - Spec-compliant minimal quoting and JSON backslash escaping (`\"`, `\\`, `\n`, `\r`, `\t`)

use serde_json::{Map, Value};
use std::collections::HashSet;

/// Supported active value delimiters for TOON tabular and inline arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToonDelimiter {
    Comma,
    Tab,
    Pipe,
}

impl ToonDelimiter {
    #[inline]
    #[must_use]
    pub const fn as_char(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Tab => '\t',
            Self::Pipe => '|',
        }
    }

    /// Header delimiter suffix placed inside `[N<suffix>]` (omitted for default comma).
    #[inline]
    #[must_use]
    pub const fn header_suffix(self) -> &'static str {
        match self {
            Self::Comma => "",
            Self::Tab => "\t",
            Self::Pipe => "|",
        }
    }
}

/// Strategy for selecting active delimiters during encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelimiterMode {
    /// Algorithmically selects the delimiter (`,`, `|`, or `\t`) per array/table that
    /// minimizes total encoded byte length and cell-quoting overhead.
    Auto,
    /// Forces a specific delimiter everywhere.
    Fixed(ToonDelimiter),
}

/// Configuration options for TOON encoding and decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToonOptions {
    /// Spaces per indentation level (spec default: 2).
    pub indent: usize,
    /// Active delimiter selection policy.
    pub delimiter: DelimiterMode,
    /// Collapses safe single-key nested object chains (`data.repo.issues[N]{...}:`).
    pub key_folding: bool,
    /// Expands unquoted dotted keys (`a.b.c`) back into nested objects during decoding.
    pub expand_folded_paths: bool,
}

impl Default for ToonOptions {
    fn default() -> Self {
        Self {
            indent: 2,
            delimiter: DelimiterMode::Auto,
            key_folding: true,
            expand_folded_paths: true,
        }
    }
}

/// Lossless Token-Oriented Object Notation (`TOON` v3) encoder and decoder.
pub struct ToonCodec;

impl ToonCodec {
    // ------------------------------------------------------------------------
    // Public High-Level Recursive Document API
    // ------------------------------------------------------------------------

    /// Encodes any arbitrary `serde_json::Value` recursively into lossless TOON v3 text.
    #[must_use]
    pub fn encode_value(value: &Value, opts: &ToonOptions) -> String {
        let mut out = String::new();
        let step = opts.indent.max(2);
        let eff_opts = ToonOptions {
            indent: step,
            ..*opts
        };
        match value {
            Value::Object(map) => {
                if map.is_empty() {
                    return "{}".to_string();
                }
                Self::write_object_entries(&mut out, map, 0, &eff_opts);
            }
            Value::Array(arr) => {
                Self::write_array(&mut out, arr, None, "", 0, &eff_opts);
            }
            scalar => {
                out.push_str(&Self::encode_scalar(scalar, ','));
            }
        }
        while out.ends_with('\n') {
            out.pop();
        }
        out
    }

    /// Parses raw JSON text and encodes it into lossless TOON v3 text.
    #[must_use]
    pub fn encode_json_str(raw_json: &str, opts: &ToonOptions) -> Option<String> {
        let parsed: Value = serde_json::from_str(raw_json.trim()).ok()?;
        Some(Self::encode_value(&parsed, opts))
    }

    /// Losslessly decodes any TOON v3 document back into `serde_json::Value` using default options
    /// (folded dotted paths expanded).
    pub fn decode_value(input: &str) -> Result<Value, String> {
        Self::decode_value_with_options(input, true)
    }

    /// Losslessly decodes any TOON v3 document back into `serde_json::Value`.
    pub fn decode_value_with_options(
        input: &str,
        expand_folded_paths: bool,
    ) -> Result<Value, String> {
        let lines = Self::collect_lines(input);
        if lines.is_empty() {
            return Ok(Value::Object(Map::new()));
        }
        if lines.len() == 1 && lines[0].content == "{}" {
            return Ok(Value::Object(Map::new()));
        }

        let mut pos = 0usize;
        let first = &lines[0];
        if let Some(arr_hdr) = Self::parse_root_array_header(first.content) {
            pos += 1;
            return Self::parse_array_body(
                &lines,
                &mut pos,
                first.indent,
                arr_hdr,
                expand_folded_paths,
            );
        }

        if Self::looks_like_object_entry(first.content) {
            let map = Self::parse_object(&lines, &mut pos, first.indent, expand_folded_paths)?;
            return Ok(Value::Object(map));
        }

        if lines.len() == 1 {
            return Self::decode_scalar_value(first.content, ',')
                .ok_or_else(|| format!("invalid root TOON scalar on line {}", first.line_no));
        }

        Err(format!(
            "unexpected TOON document structure starting at line {}",
            first.line_no
        ))
    }

    // ------------------------------------------------------------------------
    // Tabular Array Helpers (Preserving Existing `ToonCodec` Public Surface)
    // ------------------------------------------------------------------------

    /// Checks whether a slice of JSON `Value` items is eligible for `TOON` tabular encoding:
    /// - At least 3 items (2+ for internal recursive document encoding)
    /// - Every item is a non-empty flat JSON Object whose values are scalars (`Null`, `Bool`,
    ///   `Number`, or `String <= 240` chars)
    /// - Between 2 and 32 distinct columns across items with cell fill ratio `>= 70%`.
    #[must_use]
    pub fn is_eligible(items: &[Value]) -> Option<Vec<String>> {
        Self::check_tabular_eligibility(items, 3)
    }

    fn check_tabular_eligibility(items: &[Value], min_items: usize) -> Option<Vec<String>> {
        if items.len() < min_items {
            return None;
        }
        let mut columns: Vec<String> = Vec::new();
        let mut col_set: HashSet<String> = HashSet::new();
        let mut total_cells_populated = 0usize;

        for item in items {
            let obj = item.as_object()?;
            if obj.is_empty() {
                return None;
            }
            for (k, v) in obj {
                let is_scalar = match v {
                    Value::Null | Value::Bool(_) | Value::Number(_) => true,
                    Value::String(s) => s.len() <= 240,
                    Value::Array(_) | Value::Object(_) => false,
                };
                if !is_scalar {
                    return None;
                }
                if col_set.insert(k.clone()) {
                    columns.push(k.clone());
                }
                total_cells_populated += 1;
            }
        }

        if columns.len() < 2 || columns.len() > 32 {
            return None;
        }
        let total_capacity = items.len().saturating_mul(columns.len());
        if total_cells_populated.saturating_mul(100) / total_capacity.max(1) < 70 {
            return None;
        }

        Some(columns)
    }

    /// Encodes a slice of JSON objects into compact comma-delimited `TOON` row strings.
    #[must_use]
    pub fn encode_rows(items: &[Value], columns: &[String]) -> Vec<String> {
        Self::encode_rows_with_delimiter(items, columns, ToonDelimiter::Comma)
    }

    /// Encodes a slice of JSON objects into `TOON` row strings using a specific delimiter.
    #[must_use]
    pub fn encode_rows_with_delimiter(
        items: &[Value],
        columns: &[String],
        delimiter: ToonDelimiter,
    ) -> Vec<String> {
        let delim = delimiter.as_char();
        let sep = delim.to_string();
        let mut rows = Vec::with_capacity(items.len());
        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let mut cells = Vec::with_capacity(columns.len());
            for col in columns {
                let cell = match obj.get(col) {
                    None => String::new(),
                    Some(v) => Self::encode_scalar(v, delim),
                };
                cells.push(cell);
            }
            rows.push(cells.join(&sep));
        }
        rows
    }

    /// Encodes a slice of JSON objects into a standalone `TOON` block:
    /// `name[N<d>]{col1<d>col2...}:\n  v1<d>v2...`
    #[must_use]
    pub fn encode_block(name: &str, items: &[Value]) -> Option<String> {
        let columns = Self::is_eligible(items)?;
        let delim = Self::choose_table_delimiter(items, &columns, DelimiterMode::Auto);
        let sep = delim.as_char().to_string();
        let encoded_cols: Vec<String> = columns
            .iter()
            .map(|c| Self::encode_header_field(c, delim.as_char()))
            .collect();
        let rows = Self::encode_rows_with_delimiter(items, &columns, delim);
        let mut out = format!(
            "{}[{}{}]{{{}}}:\n",
            Self::encode_key(name, false),
            rows.len(),
            delim.header_suffix(),
            encoded_cols.join(&sep)
        );
        for row in rows {
            out.push_str("  ");
            out.push_str(&row);
            out.push('\n');
        }
        Some(out)
    }

    /// Losslessly decodes comma-delimited `TOON` row strings and `columns` back into `Vec<Value>`.
    #[must_use]
    pub fn decode_rows(columns: &[String], rows: &[String]) -> Vec<Value> {
        Self::decode_rows_with_delimiter(columns, rows, ToonDelimiter::Comma)
    }

    /// Losslessly decodes `TOON` row strings with the given delimiter back into `Vec<Value>`.
    #[must_use]
    pub fn decode_rows_with_delimiter(
        columns: &[String],
        rows: &[String],
        delimiter: ToonDelimiter,
    ) -> Vec<Value> {
        let delim = delimiter.as_char();
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let cells = Self::split_delimited_row(row, delim);
            let mut obj = Map::with_capacity(columns.len());
            for (idx, col) in columns.iter().enumerate() {
                if let Some(raw_cell) = cells.get(idx) {
                    if let Some(val) = Self::decode_scalar_value(raw_cell, delim) {
                        obj.insert(col.clone(), val);
                    }
                }
            }
            out.push(Value::Object(obj));
        }
        out
    }

    /// Losslessly decodes a standalone `TOON` block (`name[N<d>]{col1<d>col2...}:\n  ...`) back into `(name, Vec<Value>)`.
    #[must_use]
    pub fn decode_block(toon_block: &str) -> Option<(String, Vec<Value>)> {
        let mut lines = toon_block.lines().filter(|l| !l.trim().is_empty());
        let header_line = lines.next()?.trim();
        let (name, _quoted, hdr) = Self::parse_keyed_array_header(header_line)?;
        let cols = hdr.columns?;
        let rows: Vec<String> = lines.map(|l| l.trim().to_string()).collect();
        Some((
            name,
            Self::decode_rows_with_delimiter(&cols, &rows, hdr.delimiter),
        ))
    }

    // ------------------------------------------------------------------------
    // Internal Recursive Encoder
    // ------------------------------------------------------------------------

    fn write_object_entries(
        out: &mut String,
        map: &Map<String, Value>,
        indent: usize,
        opts: &ToonOptions,
    ) {
        let pad = " ".repeat(indent);
        for (k, v) in map {
            Self::write_object_entry(out, k, v, &pad, indent, opts);
        }
    }

    fn write_object_entry(
        out: &mut String,
        key: &str,
        val: &Value,
        line_prefix: &str,
        entry_indent: usize,
        opts: &ToonOptions,
    ) {
        // Safe key folding: collapse single-key chains of bare identifier segments (`a.b.c`)
        let (encoded_key, leaf_val) = if opts.key_folding && Self::is_foldable_segment(key) {
            let mut segments = vec![key];
            let mut cur = val;
            while segments.len() < 8 {
                if let Value::Object(inner) = cur {
                    if inner.len() == 1 {
                        if let Some((next_k, next_v)) = inner.iter().next() {
                            if Self::is_foldable_segment(next_k) {
                                segments.push(next_k.as_str());
                                cur = next_v;
                                continue;
                            }
                        }
                    }
                }
                break;
            }
            (segments.join("."), cur)
        } else {
            (Self::encode_key(key, opts.key_folding), val)
        };

        match leaf_val {
            Value::Object(child_map) => {
                if child_map.is_empty() {
                    out.push_str(line_prefix);
                    out.push_str(&encoded_key);
                    out.push_str(": {}\n");
                } else {
                    out.push_str(line_prefix);
                    out.push_str(&encoded_key);
                    out.push_str(":\n");
                    Self::write_object_entries(out, child_map, entry_indent + opts.indent, opts);
                }
            }
            Value::Array(arr) => {
                Self::write_array(
                    out,
                    arr,
                    Some(&encoded_key),
                    line_prefix,
                    entry_indent,
                    opts,
                );
            }
            scalar => {
                out.push_str(line_prefix);
                out.push_str(&encoded_key);
                out.push_str(": ");
                out.push_str(&Self::encode_scalar(scalar, ','));
                out.push('\n');
            }
        }
    }

    fn write_array(
        out: &mut String,
        arr: &[Value],
        key: Option<&str>,
        line_prefix: &str,
        entry_indent: usize,
        opts: &ToonOptions,
    ) {
        let key_part = key.unwrap_or("");
        if arr.is_empty() {
            out.push_str(line_prefix);
            out.push_str(key_part);
            out.push_str("[0]:\n");
            return;
        }

        // Case 1: Inline primitive array (all scalars, reasonable single-line width)
        let all_primitives = arr.iter().all(|v| {
            matches!(
                v,
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
            )
        });
        if all_primitives {
            let delim = Self::choose_primitive_delimiter(arr, opts.delimiter);
            let sep = delim.as_char().to_string();
            let encoded: Vec<String> = arr
                .iter()
                .map(|v| Self::encode_scalar(v, delim.as_char()))
                .collect();
            let joined = encoded.join(&sep);
            if joined.len() <= 512 {
                out.push_str(line_prefix);
                out.push_str(key_part);
                out.push('[');
                out.push_str(&arr.len().to_string());
                out.push_str(delim.header_suffix());
                out.push_str("]: ");
                out.push_str(&joined);
                out.push('\n');
                return;
            }
        }

        // Case 2: Uniform / semi-uniform tabular array of flat scalar objects
        if let Some(columns) = Self::check_tabular_eligibility(arr, 2) {
            let delim = Self::choose_table_delimiter(arr, &columns, opts.delimiter);
            let sep = delim.as_char().to_string();
            let encoded_cols: Vec<String> = columns
                .iter()
                .map(|c| Self::encode_header_field(c, delim.as_char()))
                .collect();
            out.push_str(line_prefix);
            out.push_str(key_part);
            out.push('[');
            out.push_str(&arr.len().to_string());
            out.push_str(delim.header_suffix());
            out.push_str("]{");
            out.push_str(&encoded_cols.join(&sep));
            out.push_str("}:\n");

            let row_pad = " ".repeat(entry_indent + opts.indent);
            for row in Self::encode_rows_with_delimiter(arr, &columns, delim) {
                out.push_str(&row_pad);
                out.push_str(&row);
                out.push('\n');
            }
            return;
        }

        // Case 3: Expanded list items (`- `) for nested/mixed arrays
        out.push_str(line_prefix);
        out.push_str(key_part);
        out.push('[');
        out.push_str(&arr.len().to_string());
        out.push_str("]:\n");

        let item_indent = entry_indent + opts.indent;
        let item_pad = " ".repeat(item_indent);
        let obj_indent = item_indent + 2;
        let obj_pad = " ".repeat(obj_indent);
        let bullet_prefix = format!("{item_pad}- ");

        for item in arr {
            match item {
                Value::Object(map) => {
                    if map.is_empty() {
                        out.push_str(&item_pad);
                        out.push_str("- {}\n");
                    } else {
                        for (idx, (k, v)) in map.iter().enumerate() {
                            let prefix = if idx == 0 {
                                bullet_prefix.as_str()
                            } else {
                                obj_pad.as_str()
                            };
                            Self::write_object_entry(out, k, v, prefix, obj_indent, opts);
                        }
                    }
                }
                Value::Array(nested) => {
                    Self::write_array(out, nested, None, &bullet_prefix, obj_indent, opts);
                }
                scalar => {
                    out.push_str(&bullet_prefix);
                    out.push_str(&Self::encode_scalar(scalar, ','));
                    out.push('\n');
                }
            }
        }
    }

    // ------------------------------------------------------------------------
    // Algorithmic Delimiter Selection
    // ------------------------------------------------------------------------

    fn choose_table_delimiter(
        items: &[Value],
        columns: &[String],
        mode: DelimiterMode,
    ) -> ToonDelimiter {
        if let DelimiterMode::Fixed(d) = mode {
            return d;
        }
        let score = |delim: ToonDelimiter| -> usize {
            let d = delim.as_char();
            let mut cost = if delim == ToonDelimiter::Comma { 0 } else { 1 };
            for col in columns {
                cost += Self::encode_header_field(col, d).len();
            }
            for item in items {
                if let Some(obj) = item.as_object() {
                    for col in columns {
                        if let Some(Value::String(s)) = obj.get(col) {
                            cost += Self::encode_string(s, d).len();
                        }
                    }
                }
            }
            cost
        };

        let comma_cost = score(ToonDelimiter::Comma);
        let pipe_cost = score(ToonDelimiter::Pipe);
        let tab_cost = score(ToonDelimiter::Tab);

        // Prefer `,` unless `|` or `\t` saves >= 2 bytes; prefer `|` over `\t` on ties for terminal safety
        if pipe_cost + 2 <= comma_cost && pipe_cost <= tab_cost {
            ToonDelimiter::Pipe
        } else if tab_cost + 2 <= comma_cost {
            ToonDelimiter::Tab
        } else {
            ToonDelimiter::Comma
        }
    }

    fn choose_primitive_delimiter(items: &[Value], mode: DelimiterMode) -> ToonDelimiter {
        if let DelimiterMode::Fixed(d) = mode {
            return d;
        }
        let score = |delim: ToonDelimiter| -> usize {
            let d = delim.as_char();
            let mut cost = if delim == ToonDelimiter::Comma { 0 } else { 1 };
            for item in items {
                if let Value::String(s) = item {
                    cost += Self::encode_string(s, d).len();
                }
            }
            cost
        };

        let comma_cost = score(ToonDelimiter::Comma);
        let pipe_cost = score(ToonDelimiter::Pipe);
        let tab_cost = score(ToonDelimiter::Tab);

        if pipe_cost + 2 <= comma_cost && pipe_cost <= tab_cost {
            ToonDelimiter::Pipe
        } else if tab_cost + 2 <= comma_cost {
            ToonDelimiter::Tab
        } else {
            ToonDelimiter::Comma
        }
    }

    // ------------------------------------------------------------------------
    // Quoting, Escaping & Identifier Rules (TOON v3 Spec)
    // ------------------------------------------------------------------------

    fn is_foldable_segment(s: &str) -> bool {
        let mut chars = s.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_alphabetic() || first == '_') {
            return false;
        }
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }

    fn encode_key(key: &str, key_folding: bool) -> String {
        if Self::is_foldable_segment(key) {
            return key.to_string();
        }
        if !key_folding {
            let mut chars = key.chars();
            if let Some(first) = chars.next() {
                if (first.is_ascii_alphabetic() || first == '_')
                    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                {
                    return key.to_string();
                }
            }
        }
        Self::quote_escaped(key)
    }

    fn encode_header_field(col: &str, active_delim: char) -> String {
        let safe = !col.is_empty()
            && col.trim() == col
            && !col.chars().any(|c| {
                c == active_delim
                    || matches!(c, ':' | '"' | '\\' | '[' | ']' | '{' | '}')
                    || c.is_control()
            });
        if safe {
            col.to_string()
        } else {
            Self::quote_escaped(col)
        }
    }

    fn encode_scalar(val: &Value, active_delim: char) -> String {
        match val {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    if f == 0.0 && f.is_sign_negative() {
                        return "0".to_string();
                    }
                }
                n.to_string()
            }
            Value::String(s) => Self::encode_string(s, active_delim),
            other => Self::quote_escaped(&other.to_string()),
        }
    }

    fn encode_string(s: &str, active_delim: char) -> String {
        if Self::string_needs_quotes(s, active_delim) {
            Self::quote_escaped(s)
        } else {
            s.to_string()
        }
    }

    fn string_needs_quotes(s: &str, active_delim: char) -> bool {
        if s.is_empty()
            || s.trim() != s
            || matches!(s, "true" | "false" | "null")
            || s.starts_with('-')
            || s.starts_with('#')
            || s.parse::<i64>().is_ok()
            || s.parse::<f64>().is_ok()
        {
            return true;
        }
        s.chars().any(|c| {
            c == active_delim
                || matches!(c, ':' | '"' | '\\' | '[' | ']' | '{' | '}')
                || c.is_control()
        })
    }

    fn quote_escaped(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for ch in s.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if c.is_control() => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    fn unescape_quoted(inner: &str) -> Result<String, String> {
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }
            let Some(esc) = chars.next() else {
                return Err("unterminated escape sequence in TOON string".to_string());
            };
            match esc {
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if hex.len() != 4 {
                        return Err("incomplete \\uXXXX escape in TOON string".to_string());
                    }
                    let code = u32::from_str_radix(&hex, 16)
                        .map_err(|_| format!("invalid \\u{hex} escape in TOON string"))?;
                    let decoded = char::from_u32(code)
                        .ok_or_else(|| format!("invalid Unicode scalar \\u{hex}"))?;
                    out.push(decoded);
                }
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
        }
        Ok(out)
    }

    // ------------------------------------------------------------------------
    // Internal Recursive Decoder
    // ------------------------------------------------------------------------

    fn collect_lines(input: &str) -> Vec<ToonLine<'_>> {
        let mut out = Vec::new();
        for (idx, raw) in input.lines().enumerate() {
            let trimmed_end = raw.trim_end_matches('\r');
            if trimmed_end.trim().is_empty() {
                continue;
            }
            let indent = trimmed_end.bytes().take_while(|&b| b == b' ').count();
            let content = &trimmed_end[indent..];
            out.push(ToonLine {
                indent,
                content,
                line_no: idx + 1,
            });
        }
        out
    }

    fn parse_object(
        lines: &[ToonLine<'_>],
        pos: &mut usize,
        base_indent: usize,
        expand_folded_paths: bool,
    ) -> Result<Map<String, Value>, String> {
        let mut map = Map::new();
        while *pos < lines.len() && lines[*pos].indent == base_indent {
            let line = lines[*pos];
            if line.content == "-" || line.content.starts_with("- ") {
                break;
            }
            *pos += 1;
            Self::parse_and_insert_entry(
                &mut map,
                line.content,
                base_indent,
                lines,
                pos,
                expand_folded_paths,
                line.line_no,
            )?;
        }
        Ok(map)
    }

    fn parse_and_insert_entry(
        map: &mut Map<String, Value>,
        content: &str,
        entry_indent: usize,
        lines: &[ToonLine<'_>],
        pos: &mut usize,
        expand_folded_paths: bool,
        line_no: usize,
    ) -> Result<(), String> {
        if let Some((key, was_quoted, hdr)) = Self::parse_keyed_array_header(content) {
            let arr_val =
                Self::parse_array_body(lines, pos, entry_indent, hdr, expand_folded_paths)?;
            Self::insert_key_path(map, key, was_quoted, arr_val, expand_folded_paths);
            return Ok(());
        }

        let (key, was_quoted, rest) = Self::split_key_value_line(content)
            .ok_or_else(|| format!("malformed TOON entry at line {line_no}: {content}"))?;

        let val = if rest.is_empty() {
            if *pos < lines.len() && lines[*pos].indent > entry_indent {
                let child_indent = lines[*pos].indent;
                let first_child = lines[*pos].content;
                if let Some(root_arr) = Self::parse_root_array_header(first_child) {
                    *pos += 1;
                    Self::parse_array_body(lines, pos, child_indent, root_arr, expand_folded_paths)?
                } else {
                    let child_map =
                        Self::parse_object(lines, pos, child_indent, expand_folded_paths)?;
                    Value::Object(child_map)
                }
            } else {
                Value::Object(Map::new())
            }
        } else if rest == "{}" {
            Value::Object(Map::new())
        } else {
            Self::decode_scalar_value(rest, ',')
                .ok_or_else(|| format!("invalid TOON scalar at line {line_no}: {rest}"))?
        };

        Self::insert_key_path(map, key, was_quoted, val, expand_folded_paths);
        Ok(())
    }

    fn parse_array_body(
        lines: &[ToonLine<'_>],
        pos: &mut usize,
        header_indent: usize,
        hdr: ArrayHeader,
        expand_folded_paths: bool,
    ) -> Result<Value, String> {
        if let Some(inline) = hdr.inline_values {
            let cells = Self::split_delimited_row(&inline, hdr.delimiter.as_char());
            let mut items = Vec::with_capacity(cells.len());
            for cell in cells {
                let val = Self::decode_scalar_value(&cell, hdr.delimiter.as_char())
                    .unwrap_or(Value::Null);
                items.push(val);
            }
            return Ok(Value::Array(items));
        }

        if hdr.declared_len == 0 {
            return Ok(Value::Array(Vec::new()));
        }

        if let Some(columns) = hdr.columns {
            let mut rows = Vec::new();
            while *pos < lines.len() && lines[*pos].indent > header_indent {
                rows.push(lines[*pos].content.to_string());
                *pos += 1;
            }
            return Ok(Value::Array(Self::decode_rows_with_delimiter(
                &columns,
                &rows,
                hdr.delimiter,
            )));
        }

        // Expanded `- ` list items
        if *pos >= lines.len() || lines[*pos].indent <= header_indent {
            return Ok(Value::Array(Vec::new()));
        }
        let item_indent = lines[*pos].indent;
        let mut items = Vec::new();
        while *pos < lines.len() && lines[*pos].indent == item_indent {
            let line = lines[*pos];
            let Some(rest) = line
                .content
                .strip_prefix("- ")
                .or_else(|| (line.content == "-").then_some(""))
            else {
                break;
            };
            *pos += 1;
            let obj_indent = item_indent + 2;

            if rest.is_empty() {
                if *pos < lines.len() && lines[*pos].indent > item_indent {
                    let child_indent = lines[*pos].indent;
                    let child_map =
                        Self::parse_object(lines, pos, child_indent, expand_folded_paths)?;
                    items.push(Value::Object(child_map));
                } else {
                    items.push(Value::Null);
                }
            } else if rest == "{}" {
                items.push(Value::Object(Map::new()));
            } else if let Some(nested_hdr) = Self::parse_root_array_header(rest) {
                let nested_arr = Self::parse_array_body(
                    lines,
                    pos,
                    obj_indent,
                    nested_hdr,
                    expand_folded_paths,
                )?;
                items.push(nested_arr);
            } else if Self::looks_like_object_entry(rest) {
                let mut item_map = Map::new();
                Self::parse_and_insert_entry(
                    &mut item_map,
                    rest,
                    obj_indent,
                    lines,
                    pos,
                    expand_folded_paths,
                    line.line_no,
                )?;
                while *pos < lines.len() && lines[*pos].indent == obj_indent {
                    let sib = lines[*pos];
                    if sib.content == "-" || sib.content.starts_with("- ") {
                        break;
                    }
                    *pos += 1;
                    Self::parse_and_insert_entry(
                        &mut item_map,
                        sib.content,
                        obj_indent,
                        lines,
                        pos,
                        expand_folded_paths,
                        sib.line_no,
                    )?;
                }
                items.push(Value::Object(item_map));
            } else {
                let val = Self::decode_scalar_value(rest, ',').ok_or_else(|| {
                    format!("invalid list scalar at line {}: {rest}", line.line_no)
                })?;
                items.push(val);
            }
        }
        Ok(Value::Array(items))
    }

    fn insert_key_path(
        map: &mut Map<String, Value>,
        key: String,
        was_quoted: bool,
        val: Value,
        expand_folded_paths: bool,
    ) {
        if !expand_folded_paths || was_quoted || !key.contains('.') {
            map.insert(key, val);
            return;
        }
        let segments: Vec<&str> = key.split('.').collect();
        if segments.iter().any(|s| s.is_empty()) {
            map.insert(key, val);
            return;
        }
        Self::insert_segments(map, &segments, val);
    }

    fn insert_segments(map: &mut Map<String, Value>, segments: &[&str], val: Value) {
        if segments.len() == 1 {
            map.insert(segments[0].to_string(), val);
            return;
        }
        let head = segments[0].to_string();
        let entry = map.entry(head).or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        if let Value::Object(child_map) = entry {
            Self::insert_segments(child_map, &segments[1..], val);
        }
    }

    fn looks_like_object_entry(content: &str) -> bool {
        Self::parse_keyed_array_header(content).is_some()
            || Self::split_key_value_line(content).is_some()
    }

    /// Parses leading key token (either quoted `"..."` or unquoted up to `:` or `[`).
    /// Returns `(decoded_key, was_quoted, remainder_slice)`.
    fn parse_leading_key(content: &str) -> Option<(String, bool, &str)> {
        if let Some(after_quote) = content.strip_prefix('"') {
            let mut escaped = false;
            for (idx, ch) in after_quote.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    let inner = &after_quote[..idx];
                    let decoded = Self::unescape_quoted(inner).ok()?;
                    let rest = &after_quote[idx + 1..];
                    return Some((decoded, true, rest));
                }
            }
            return None;
        }

        // Unquoted key ends at first `:` or `[`
        let end = content.find([':', '['])?;
        let raw_key = &content[..end];
        if raw_key.is_empty()
            || raw_key.contains(char::is_whitespace)
            || raw_key.contains('/')
            || raw_key.starts_with('-')
        {
            return None;
        }
        Some((raw_key.to_string(), false, &content[end..]))
    }

    fn split_key_value_line(content: &str) -> Option<(String, bool, &str)> {
        let (key, was_quoted, rest) = Self::parse_leading_key(content)?;
        let after_colon = rest.strip_prefix(':')?;
        if !after_colon.is_empty() && !after_colon.starts_with(' ') {
            return None;
        }
        Some((key, was_quoted, after_colon.trim_start_matches(' ')))
    }

    fn parse_root_array_header(content: &str) -> Option<ArrayHeader> {
        if !content.starts_with('[') {
            return None;
        }
        Self::parse_bracket_array_suffix(content)
    }

    fn parse_keyed_array_header(content: &str) -> Option<(String, bool, ArrayHeader)> {
        let (key, was_quoted, rest) = Self::parse_leading_key(content)?;
        if !rest.starts_with('[') {
            return None;
        }
        let hdr = Self::parse_bracket_array_suffix(rest)?;
        Some((key, was_quoted, hdr))
    }

    /// Parses `[N<delim>]{col1,col2}:` or `[N<delim>]: <inline>` or `[N<delim>]:`.
    fn parse_bracket_array_suffix(s: &str) -> Option<ArrayHeader> {
        let rest = s.strip_prefix('[')?;
        let close_bracket = rest.find(']')?;
        let len_spec = &rest[..close_bracket];
        let (len_digits, delimiter) = if let Some(d) = len_spec.strip_suffix('|') {
            (d, ToonDelimiter::Pipe)
        } else if let Some(d) = len_spec.strip_suffix('\t') {
            (d, ToonDelimiter::Tab)
        } else {
            (len_spec, ToonDelimiter::Comma)
        };
        let declared_len: usize = len_digits.parse().ok()?;
        let after_bracket = &rest[close_bracket + 1..];

        if let Some(after_brace) = after_bracket.strip_prefix('{') {
            // Scan to matching unquoted `}`
            let mut in_quotes = false;
            let mut escaped = false;
            let mut close_brace = None;
            for (idx, ch) in after_brace.char_indices() {
                if in_quotes {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        in_quotes = false;
                    }
                } else if ch == '"' {
                    in_quotes = true;
                } else if ch == '}' {
                    close_brace = Some(idx);
                    break;
                }
            }
            let close_idx = close_brace?;
            let cols_raw = &after_brace[..close_idx];
            let tail = after_brace[close_idx + 1..].trim();
            if tail != ":" {
                return None;
            }
            let columns: Vec<String> = Self::split_delimited_row(cols_raw, delimiter.as_char())
                .into_iter()
                .map(|c| {
                    let t = c.trim();
                    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
                        Self::unescape_quoted(&t[1..t.len() - 1]).unwrap_or_else(|_| t.to_string())
                    } else {
                        t.to_string()
                    }
                })
                .filter(|c| !c.is_empty())
                .collect();
            if columns.is_empty() {
                return None;
            }
            return Some(ArrayHeader {
                declared_len,
                delimiter,
                columns: Some(columns),
                inline_values: None,
            });
        }

        let after_colon = after_bracket.strip_prefix(':')?;
        if after_colon.is_empty() {
            return Some(ArrayHeader {
                declared_len,
                delimiter,
                columns: None,
                inline_values: None,
            });
        }
        let inline = after_colon.strip_prefix(' ')?;
        Some(ArrayHeader {
            declared_len,
            delimiter,
            columns: None,
            inline_values: Some(inline.to_string()),
        })
    }

    fn split_delimited_row(row: &str, delim: char) -> Vec<String> {
        let mut cells = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut escaped = false;

        for ch in row.chars() {
            if in_quotes {
                current.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_quotes = false;
                }
            } else if ch == '"' && current.is_empty() {
                in_quotes = true;
                current.push(ch);
            } else if ch == delim {
                cells.push(std::mem::take(&mut current));
            } else {
                current.push(ch);
            }
        }
        cells.push(current);
        cells
    }

    fn decode_scalar_value(raw_cell: &str, _active_delim: char) -> Option<Value> {
        if raw_cell.is_empty() {
            return None;
        }
        if raw_cell.starts_with('"') && raw_cell.ends_with('"') && raw_cell.len() >= 2 {
            let inner = &raw_cell[1..raw_cell.len() - 1];
            return Self::unescape_quoted(inner).ok().map(Value::String);
        }
        match raw_cell {
            "null" => Some(Value::Null),
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            other => {
                if let Ok(i) = other.parse::<i64>() {
                    return Some(Value::Number(serde_json::Number::from(i)));
                }
                if let Ok(u) = other.parse::<u64>() {
                    return Some(Value::Number(serde_json::Number::from(u)));
                }
                if let Ok(f) = other.parse::<f64>() {
                    if let Some(num) = serde_json::Number::from_f64(f) {
                        return Some(Value::Number(num));
                    }
                }
                Some(Value::String(other.to_string()))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ToonLine<'a> {
    indent: usize,
    content: &'a str,
    line_no: usize,
}

#[derive(Debug, Clone)]
struct ArrayHeader {
    declared_len: usize,
    delimiter: ToonDelimiter,
    columns: Option<Vec<String>>,
    inline_values: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_toon_v3_full_nested_document_lossless_roundtrip_and_key_folding() {
        let doc = json!({
            "data": {
                "repository": {
                    "name": "minicode",
                    "stars": 4200,
                    "tags": ["rust", "tui", "agent"],
                    "issues": [
                        {"id": 1, "title": "Fix parser: edge case", "open": true, "author": "alice"},
                        {"id": 2, "title": "Add TOON v3", "open": true, "author": "bob"},
                        {"id": 3, "title": "Line1\nLine2", "open": false, "author": "carol"}
                    ]
                }
            },
            "meta": {
                "empty_obj": {},
                "empty_arr": [],
                "literal.dotted": "must not fold",
                "num_as_str": "042",
                "bool_as_str": "true",
                "dash_str": "-item",
                "empty_str": ""
            }
        });

        let encoded = ToonCodec::encode_value(&doc, &ToonOptions::default());
        // Safe key folding collapses `data.repository`
        assert!(
            encoded.contains("data.repository:"),
            "expected key folding for single-key chain, got:\n{encoded}"
        );
        // Literal dotted key is quoted so round-trip never splits it into nested objects
        assert!(
            encoded.contains("\"literal.dotted\": must not fold"),
            "expected quoted literal dotted key, got:\n{encoded}"
        );
        // Inline primitive array
        assert!(
            encoded.contains("tags[3]: rust,tui,agent"),
            "expected inline primitive array, got:\n{encoded}"
        );
        // Tabular array header
        assert!(
            encoded.contains("issues[3]{id,title,open,author}:"),
            "expected tabular array header, got:\n{encoded}"
        );

        let decoded = ToonCodec::decode_value(&encoded).expect("decode must succeed");
        assert_eq!(decoded, doc);
    }

    #[test]
    fn test_toon_v3_algorithmic_delimiter_selection_pipe_and_tab_roundtrip() {
        // Every row has commas -> Auto delimiter must pick `|` instead of quoting every cell!
        let doc = json!({
            "locations": [
                {"id": 1, "city": "San Francisco, CA", "notes": "HQ, West"},
                {"id": 2, "city": "Austin, TX", "notes": "Ops, Central"},
                {"id": 3, "city": "New York, NY", "notes": "Sales, East"}
            ]
        });

        let auto_encoded = ToonCodec::encode_value(&doc, &ToonOptions::default());
        assert!(
            auto_encoded.contains("locations[3|]{id|city|notes}:"),
            "Auto delimiter should pick pipe `|` for comma-heavy rows, got:\n{auto_encoded}"
        );
        assert!(auto_encoded.contains("1|San Francisco, CA|HQ, West"));
        assert_eq!(
            ToonCodec::decode_value(&auto_encoded).expect("pipe decode"),
            doc
        );

        // Explicit Tab delimiter mode (`[3\t]{id\tcity\tnotes}:`)
        let tab_opts = ToonOptions {
            delimiter: DelimiterMode::Fixed(ToonDelimiter::Tab),
            ..ToonOptions::default()
        };
        let tab_encoded = ToonCodec::encode_value(&doc, &tab_opts);
        assert!(tab_encoded.contains("locations[3\t]{id\tcity\tnotes}:"));
        assert_eq!(
            ToonCodec::decode_value(&tab_encoded).expect("tab decode"),
            doc
        );
    }

    #[test]
    fn test_toon_v3_mixed_nested_list_items_and_semi_uniform_absent_keys() {
        let doc = json!({
            "events": [
                {
                    "id": 1,
                    "actor": {"login": "alice", "admin": true},
                    "labels": ["bug", "p1"]
                },
                {
                    "id": 2,
                    "actor": {"login": "bob", "admin": false},
                    "labels": []
                }
            ],
            "metrics": [
                {"id": 1, "status": "ok", "ms": 12.5},
                {"id": 2, "status": "error", "ms": 950.0, "error": "timeout"},
                {"id": 3, "status": "ok", "ms": 14.0, "error": ""},
                {"id": 4, "status": "ok", "ms": 11.0, "error": null}
            ]
        });

        let encoded = ToonCodec::encode_value(&doc, &ToonOptions::default());
        // `events` uses `- ` list items with nested inline arrays
        assert!(encoded.contains("events[2]:"));
        assert!(encoded.contains("  - id: 1"));
        assert!(encoded.contains("    labels[2]: bug,p1"));
        // `metrics` uses tabular encoding preserving absent vs empty string vs null losslessly!
        assert!(encoded.contains("metrics[4]{id,status,ms,error}:"));

        let decoded = ToonCodec::decode_value(&encoded).expect("mixed decode");
        assert_eq!(decoded, doc);
    }

    #[test]
    fn test_toon_v3_token_savings_vs_pretty_json() {
        let rows: Vec<Value> = (1..=30)
            .map(|i| {
                json!({
                    "id": i,
                    "service": format!("worker-{i}"),
                    "region": "us-east-1",
                    "status": "healthy",
                    "cpu_pct": 12.5 + (i as f64),
                    "mem_mb": 256 + i * 8
                })
            })
            .collect();
        let doc = json!({"cluster": {"nodes": rows}});
        let pretty_json = serde_json::to_string_pretty(&doc).expect("pretty json");
        let toon = ToonCodec::encode_value(&doc, &ToonOptions::default());
        assert!(
            toon.len() * 100 < pretty_json.len() * 45,
            "TOON ({}B) should save >55% bytes vs pretty JSON ({}B)",
            toon.len(),
            pretty_json.len()
        );
        assert_eq!(ToonCodec::decode_value(&toon).expect("roundtrip"), doc);
    }
}
