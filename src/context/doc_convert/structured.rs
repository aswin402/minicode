//! Pure-Rust Structured Text Converters (zero regexes, zero hardcoded extension walls):
//! - [`IpynbConverter`] (priority 20): Jupyter Notebook JSON -> Markdown + code cells + pruned outputs
//!   (strips giant base64 PNG/HTML blobs automatically).
//! - [`FeedXmlConverter`] (priority 25): RSS 2.0 & Atom 1.0 XML feeds -> Markdown article digest.
//! - [`DelimitedTableConverter`] (priority 35): Statistical minimum-column-variance delimiter solver
//!   + RFC 4180 FSM -> Markdown pipe tables (CSV/TSV/PSV/SSV).
//! - [`HtmlDocumentConverter`] (priority 40): HTML -> Fit Markdown via `SmartMarkdownExtractor`.

use super::ooxml::{escape_md_cell, render_bounded_table};
use super::xml::parse_xml_tree;
use super::{ConvertedDocument, DocumentConverter, StreamInfo};
use crate::error::{ContextError, Result};
use serde_json::Value;

const MAX_TABLE_RENDER_ROWS: usize = 80;
const MAX_CELL_OUTPUT_CHARS: usize = 1200;

// ============================================================================
// 1. Jupyter Notebook Converter (`.ipynb`, Priority 20)
// ============================================================================

pub struct IpynbConverter;

impl DocumentConverter for IpynbConverter {
    fn name(&self) -> &'static str {
        "ipynb"
    }

    fn priority(&self) -> u16 {
        20
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        info.mime_type == "application/x-ipynb+json"
            || (info.is_utf8_text && info.extension.as_deref() == Some("ipynb"))
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let root: Value = serde_json::from_slice(bytes)
            .map_err(|e| ContextError::Parse(format!("Invalid Jupyter Notebook JSON: {}", e)))?;

        let lang = root
            .pointer("/metadata/kernelspec/language")
            .or_else(|| root.pointer("/metadata/language_info/name"))
            .and_then(Value::as_str)
            .unwrap_or("python");

        let cells = root
            .get("cells")
            .and_then(Value::as_array)
            .ok_or_else(|| ContextError::Parse("Jupyter Notebook missing `cells` array".into()))?;

        let mut sections = Vec::new();
        let mut title: Option<String> = None;

        for (idx, cell) in cells.iter().enumerate() {
            let cell_type = cell.get("cell_type").and_then(Value::as_str).unwrap_or("");
            let source = join_json_string_or_lines(cell.get("source"));
            if source.trim().is_empty() && cell_type != "code" {
                continue;
            }

            match cell_type {
                "markdown" => {
                    if title.is_none() {
                        for line in source.lines() {
                            if let Some(h) = line.trim().strip_prefix("# ") {
                                let t = h.trim();
                                if !t.is_empty() {
                                    title = Some(t.to_string());
                                    break;
                                }
                            }
                        }
                    }
                    sections.push(source.trim_end().to_string());
                }
                "code" => {
                    let exec_badge = cell
                        .get("execution_count")
                        .and_then(Value::as_u64)
                        .map(|n| format!(" [In {}]", n))
                        .unwrap_or_default();
                    let mut block = format!(
                        "## Cell {} ({}){}\n\n```{}\n{}\n```",
                        idx + 1,
                        lang,
                        exec_badge,
                        lang,
                        source.trim_end()
                    );

                    if let Some(outputs) = cell.get("outputs").and_then(Value::as_array) {
                        let rendered = render_ipynb_outputs(outputs);
                        if !rendered.is_empty() {
                            block.push_str("\n\n**Output:**\n");
                            block.push_str(&rendered);
                        }
                    }
                    sections.push(block);
                }
                "raw" => {
                    sections.push(format!("```text\n{}\n```", source.trim_end()));
                }
                _ => {}
            }
        }

        let markdown = if sections.is_empty() {
            "*[Empty Jupyter Notebook]*".to_string()
        } else {
            sections.join("\n\n")
        };

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: title.or_else(|| info.source_name.clone()),
            markdown,
        })
    }
}

fn join_json_string_or_lines(val: Option<&Value>) -> String {
    match val {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn render_ipynb_outputs(outputs: &[Value]) -> String {
    let mut parts = Vec::new();
    for out in outputs {
        let out_type = out.get("output_type").and_then(Value::as_str).unwrap_or("");
        match out_type {
            "stream" => {
                let text = join_json_string_or_lines(out.get("text"));
                let clean = truncate_utf8(text.trim(), MAX_CELL_OUTPUT_CHARS);
                if !clean.is_empty() {
                    parts.push(format!("```text\n{}\n```", clean));
                }
            }
            "error" => {
                let ename = out.get("ename").and_then(Value::as_str).unwrap_or("Error");
                let evalue = out.get("evalue").and_then(Value::as_str).unwrap_or("");
                parts.push(format!("`{}: {}`", ename, evalue));
            }
            "execute_result" | "display_data" => {
                if let Some(data) = out.get("data").and_then(Value::as_object) {
                    if let Some(plain) = data.get("text/plain") {
                        let txt = join_json_string_or_lines(Some(plain));
                        let clean = truncate_utf8(txt.trim(), MAX_CELL_OUTPUT_CHARS);
                        if !clean.is_empty() {
                            parts.push(format!("```text\n{}\n```", clean));
                        }
                    }
                    // Strip bloated base64 raster/SVG/HTML outputs into compact size markers
                    for (mime, payload) in data {
                        if mime.starts_with("image/") || mime == "text/html" {
                            let approx_bytes = match payload {
                                Value::String(s) => s.len(),
                                Value::Array(arr) => {
                                    arr.iter().filter_map(Value::as_str).map(str::len).sum()
                                }
                                _ => 0,
                            };
                            if approx_bytes > 256 {
                                parts.push(format!(
                                    "*[omitted rich output `{}` (~{:.1} KB)]*",
                                    mime,
                                    approx_bytes as f64 / 1024.0
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    parts.join("\n")
}

// ============================================================================
// 2. RSS 2.0 & Atom 1.0 XML Feed Converter (Priority 25)
// ============================================================================

pub struct FeedXmlConverter;

impl DocumentConverter for FeedXmlConverter {
    fn name(&self) -> &'static str {
        "feed_xml"
    }

    fn priority(&self) -> u16 {
        25
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        matches!(
            info.mime_type.as_str(),
            "application/rss+xml" | "application/atom+xml"
        )
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let raw = String::from_utf8_lossy(bytes);
        let root = parse_xml_tree(&raw)
            .ok_or_else(|| ContextError::Parse("Malformed RSS/Atom XML feed".into()))?;

        let mut md = String::new();
        let feed_title;

        if root.local() == "feed" {
            // Atom 1.0
            feed_title = root
                .first_child("title")
                .map(|n| n.collect_text().trim().to_string())
                .filter(|s| !s.is_empty());
            md.push_str(&format!(
                "# {}\n\n",
                feed_title.as_deref().unwrap_or("Atom Feed")
            ));

            for entry in root.child_elements("entry").take(60) {
                let etitle = entry
                    .first_child("title")
                    .map(|n| n.collect_text().trim().to_string())
                    .unwrap_or_else(|| "Untitled".to_string());
                let link = entry
                    .child_elements("link")
                    .find_map(|l| l.attr("href"))
                    .unwrap_or("");
                let date = entry
                    .first_child("updated")
                    .or_else(|| entry.first_child("published"))
                    .map(|n| n.collect_text().trim().to_string())
                    .unwrap_or_default();
                let summary_raw = entry
                    .first_child("summary")
                    .or_else(|| entry.first_child("content"))
                    .map(|n| n.collect_text())
                    .unwrap_or_default();
                let summary = distill_inline_html(&summary_raw);

                md.push_str(&format!("## {}\n\n", etitle));
                if !date.is_empty() {
                    md.push_str(&format!("- **Updated:** {}\n", date));
                }
                if !link.is_empty() {
                    md.push_str(&format!("- **Link:** {}\n", link));
                }
                if !summary.is_empty() {
                    md.push_str(&format!("\n{}\n", summary));
                }
                md.push('\n');
            }
        } else {
            // RSS 2.0 / RDF
            let channel = root.first_child("channel").unwrap_or(&root);
            feed_title = channel
                .first_child("title")
                .map(|n| n.collect_text().trim().to_string())
                .filter(|s| !s.is_empty());
            md.push_str(&format!(
                "# {}\n\n",
                feed_title.as_deref().unwrap_or("RSS Feed")
            ));

            let mut items = Vec::new();
            channel.find_all("item", &mut items);
            for item in items.into_iter().take(60) {
                let ititle = item
                    .first_child("title")
                    .map(|n| n.collect_text().trim().to_string())
                    .unwrap_or_else(|| "Untitled".to_string());
                let link = item
                    .first_child("link")
                    .map(|n| n.collect_text().trim().to_string())
                    .unwrap_or_default();
                let date = item
                    .first_child("pubDate")
                    .or_else(|| item.first_child("date"))
                    .map(|n| n.collect_text().trim().to_string())
                    .unwrap_or_default();
                let desc_raw = item
                    .first_child("encoded")
                    .or_else(|| item.first_child("description"))
                    .map(|n| n.collect_text())
                    .unwrap_or_default();
                let desc = distill_inline_html(&desc_raw);

                md.push_str(&format!("## {}\n\n", ititle));
                if !date.is_empty() {
                    md.push_str(&format!("- **Published:** {}\n", date));
                }
                if !link.is_empty() {
                    md.push_str(&format!("- **Link:** {}\n", link));
                }
                if !desc.is_empty() {
                    md.push_str(&format!("\n{}\n", desc));
                }
                md.push('\n');
            }
        }

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: feed_title,
            markdown: md.trim_end().to_string(),
        })
    }
}

fn distill_inline_html(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.contains('<') && trimmed.contains('>') {
        let md = crate::tools::browser::markdown::SmartMarkdownExtractor::extract_fit_markdown(
            trimmed, None,
        );
        truncate_utf8(md.trim(), 1200)
    } else {
        truncate_utf8(trimmed, 1200)
    }
}

// ============================================================================
// 3. Statistical Delimited Table Converter (CSV/TSV/PSV, Priority 35)
// ============================================================================

/// Statistically infers the most likely field delimiter among `[',', '\t', ';', '|']` by
/// minimizing normalized column-count variance across the first 25 non-empty lines while
/// rejecting prose or source code (zero regexes, zero extension hardcoding).
#[must_use]
pub fn infer_tabular_delimiter(text: &str) -> Option<char> {
    const CANDIDATES: [char; 4] = [',', '\t', ';', '|'];
    let sample_lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .take(25)
        .collect();
    if sample_lines.len() < 2 {
        return None;
    }

    // Reject source code / JSON / Markdown / XML blocks immediately via structural invariants
    let first = sample_lines[0].trim_start();
    if first.starts_with('{')
        || first.starts_with('[')
        || first.starts_with('<')
        || first.starts_with('#')
        || first.starts_with("//")
        || first.starts_with("/*")
        || first.starts_with("use ")
        || first.starts_with("pub ")
        || first.starts_with("fn ")
        || first.starts_with("import ")
        || first.starts_with("def ")
        || first.starts_with("class ")
    {
        return None;
    }

    let mut best: Option<(char, f64)> = None;
    for delim in CANDIDATES {
        let counts: Vec<usize> = sample_lines
            .iter()
            .map(|l| parse_rfc4180_row(l, delim).len())
            .collect();
        let min_c = counts.iter().copied().min().unwrap_or(0);
        let max_c = counts.iter().copied().max().unwrap_or(0);
        if min_c < 2 || max_c > 128 {
            continue;
        }
        let mean = counts.iter().sum::<usize>() as f64 / counts.len() as f64;
        let variance = counts
            .iter()
            .map(|&c| {
                let d = c as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / counts.len() as f64;

        // Require tight structural consistency (normalized std-dev <= 0.15 across sample rows)
        if variance.sqrt() / mean > 0.15 {
            continue;
        }
        // Score prefers zero variance, then higher column richness
        let score = mean / (1.0 + variance * 10.0);
        if best.is_none_or(|(_, best_s)| score > best_s) {
            best = Some((delim, score));
        }
    }

    best.map(|(d, _)| d)
}

/// Deterministic RFC 4180 finite-state machine parsing a single delimited row (supporting quoted
/// fields and `""` escaped quotes).
#[must_use]
pub fn parse_rfc4180_row(line: &str, delim: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(ch);
            }
        } else if ch == '"' && cur.is_empty() {
            in_quotes = true;
        } else if ch == delim {
            fields.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

pub struct DelimitedTableConverter;

impl DocumentConverter for DelimitedTableConverter {
    fn name(&self) -> &'static str {
        "delimited_table"
    }

    fn priority(&self) -> u16 {
        35
    }

    fn passthrough_when_editable(&self) -> bool {
        true
    }

    fn accepts(&self, bytes: &[u8], info: &StreamInfo) -> bool {
        if !info.is_utf8_text {
            return false;
        }
        if matches!(
            info.mime_type.as_str(),
            "text/csv" | "text/tab-separated-values"
        ) {
            return true;
        }
        let sample = std::str::from_utf8(&bytes[..bytes.len().min(8192)]).unwrap_or("");
        infer_tabular_delimiter(sample).is_some()
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let text = String::from_utf8_lossy(bytes);
        let delim = infer_tabular_delimiter(&text).unwrap_or(',');
        let mut rows: Vec<Vec<String>> = Vec::new();

        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parsed: Vec<String> = parse_rfc4180_row(line, delim)
                .into_iter()
                .map(|c| escape_md_cell(&c))
                .collect();
            if !parsed.is_empty() {
                rows.push(parsed);
            }
        }

        let label = info.source_name.as_deref().unwrap_or("table.csv");
        let mut md = format!("# Table: `{}` ({} rows)\n\n", label, rows.len());
        md.push_str(&render_bounded_table(&rows, MAX_TABLE_RENDER_ROWS));

        // Append automatic numeric column statistics when >= 4 data rows exist
        if rows.len() >= 5 {
            let headers = &rows[0];
            let mut stats = Vec::new();
            for (col_idx, hdr) in headers.iter().enumerate() {
                let nums: Vec<f64> = rows[1..]
                    .iter()
                    .filter_map(|r| r.get(col_idx))
                    .filter_map(|v| v.replace(',', "").parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .collect();
                if nums.len() >= 4 && nums.len() * 2 >= rows.len() - 1 {
                    let min_v = nums.iter().copied().fold(f64::INFINITY, f64::min);
                    let max_v = nums.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    let mean_v = nums.iter().sum::<f64>() / nums.len() as f64;
                    stats.push(format!(
                        "`{}`(min={:.2}, max={:.2}, mean={:.2})",
                        hdr, min_v, max_v, mean_v
                    ));
                }
            }
            if !stats.is_empty() {
                md.push_str(&format!("\n\n**Numeric Summary:** {}", stats.join(", ")));
            }
        }

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: Some(label.to_string()),
            markdown: md,
        })
    }
}

// ============================================================================
// 4. HTML Document Converter (Priority 40)
// ============================================================================

pub struct HtmlDocumentConverter;

impl DocumentConverter for HtmlDocumentConverter {
    fn name(&self) -> &'static str {
        "html"
    }

    fn priority(&self) -> u16 {
        40
    }

    fn passthrough_when_editable(&self) -> bool {
        true
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        matches!(
            info.mime_type.as_str(),
            "text/html" | "application/xhtml+xml"
        )
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let html = String::from_utf8_lossy(bytes);
        let title = crate::tools::crawler::markdown::MarkdownDistiller::extract_title(&html);
        let fit_md = crate::tools::browser::markdown::SmartMarkdownExtractor::extract_fit_markdown(
            &html, None,
        );
        let clean_title = (title != "Untitled Page").then_some(title);
        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: clean_title,
            markdown: fit_md,
        })
    }
}

#[inline]
fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

#[cfg(test)]
mod tests {
    use crate::context::doc_convert::{ConversionPolicy, DocumentConverterRegistry};

    #[test]
    fn test_ipynb_converter_strips_base64_image_bloat_and_renders_cells() {
        let huge_png = "A".repeat(4000);
        let nb = serde_json::json!({
            "nbformat": 4,
            "metadata": { "kernelspec": { "language": "python" } },
            "cells": [
                { "cell_type": "markdown", "source": ["# Analysis\n", "Key findings."] },
                {
                    "cell_type": "code",
                    "execution_count": 3,
                    "source": ["import math\n", "print(math.pi)"],
                    "outputs": [
                        { "output_type": "stream", "text": ["3.14159\n"] },
                        { "output_type": "display_data", "data": { "image/png": huge_png } }
                    ]
                }
            ]
        })
        .to_string();

        let doc = DocumentConverterRegistry::global()
            .convert_bytes(
                nb.as_bytes(),
                Some("exp.ipynb"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .unwrap();
        assert_eq!(doc.converter_name, "ipynb");
        assert_eq!(doc.title.as_deref(), Some("Analysis"));
        assert!(doc.markdown.contains("## Cell 2 (python) [In 3]"));
        assert!(doc.markdown.contains("3.14159"));
        assert!(doc.markdown.contains("omitted rich output `image/png`"));
        assert!(
            doc.markdown.len() < 1000,
            "Base64 PNG bloat must be stripped"
        );
    }

    #[test]
    fn test_rss_and_statistical_csv_converters() {
        let reg = DocumentConverterRegistry::global();

        let rss = br#"<?xml version="1.0"?>
        <rss version="2.0"><channel>
          <title>Release Feed</title>
          <item>
            <title>v0.6.5 Shipped</title>
            <link>https://example.com/v065</link>
            <pubDate>Fri, 27 Nov 2026 00:00:00 GMT</pubDate>
            <description><![CDATA[<p>Added <b>pure Rust</b> converters.</p>]]></description>
          </item>
        </channel></rss>"#;
        let rss_doc = reg
            .convert_bytes(rss, Some("feed.xml"), None, ConversionPolicy::LocalEditable)
            .unwrap()
            .unwrap();
        assert_eq!(rss_doc.converter_name, "feed_xml");
        assert!(rss_doc.markdown.contains("# Release Feed"));
        assert!(rss_doc.markdown.contains("## v0.6.5 Shipped"));
        assert!(rss_doc.markdown.contains("pure Rust"));

        // Statistical delimiter solver detects semicolon-delimited table with zero extension hint!
        let ssv = b"region;users;latency\nus-east;100;12.5\nus-west;250;15.0\neu-west;180;18.2\nap-south;90;24.0\nna-north;140;11.1\n";
        // Editable local read leaves CSV verbatim
        assert!(reg
            .convert_bytes(
                ssv,
                Some("metrics.dat"),
                None,
                ConversionPolicy::LocalEditable
            )
            .unwrap()
            .is_none());
        // StructuralOutline & WebOrStream convert to Markdown table + numeric summary!
        let tbl_doc = reg
            .convert_bytes(
                ssv,
                Some("metrics.dat"),
                None,
                ConversionPolicy::StructuralOutline,
            )
            .unwrap()
            .unwrap();
        assert_eq!(tbl_doc.converter_name, "delimited_table");
        assert!(tbl_doc.markdown.contains("| region | users | latency |"));
        assert!(tbl_doc.markdown.contains("Numeric Summary:"));
    }
}
