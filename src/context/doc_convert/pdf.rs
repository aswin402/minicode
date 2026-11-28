//! Pure-Rust zero-dependency PDF Structural Text & Layout Extractor (`PdfConverter`, priority 15).
//!
//! Parses PDF indirect objects, decompresses `/FlateDecode` (RFC 1950 zlib via `deflate.rs`),
//! `/ASCIIHexDecode`, and `/ASCII85Decode` streams, decodes `/ToUnicode` CMap hex-to-UTF-16BE
//! tables (`beginbfchar` / `beginbfrange`), executes a deterministic PostScript text-operator
//! state machine (`BT`/`ET`, `Tf`, `Td`/`TD`/`Tm`/`T*`/`TL`, `Tj`/`TJ`/`'`/`"`), and clusters
//! positioned text spans by 2D `(y, x)` coordinates into reading-order paragraphs and headings.

use super::deflate::{inflate_zlib, DEFAULT_MAX_INFLATE_BYTES};
use super::{ConvertedDocument, DocumentConverter, StreamInfo};
use crate::error::Result;
use std::collections::HashMap;

pub struct PdfConverter;

impl DocumentConverter for PdfConverter {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn priority(&self) -> u16 {
        15
    }

    fn accepts(&self, bytes: &[u8], info: &StreamInfo) -> bool {
        info.mime_type == "application/pdf" || bytes.starts_with(b"%PDF-")
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let extracted = extract_pdf_markdown(bytes, info.source_name.as_deref());
        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: extracted.title,
            markdown: extracted.markdown,
        })
    }
}

struct PdfExtraction {
    title: Option<String>,
    markdown: String,
}

#[derive(Debug, Clone)]
struct PositionedSpan {
    page_idx: usize,
    x: f64,
    y: f64,
    font_size: f64,
    text: String,
}

/// Decodes all `/ToUnicode` CMaps and content streams across a PDF byte buffer into Markdown.
fn extract_pdf_markdown(bytes: &[u8], source_name: Option<&str>) -> PdfExtraction {
    let title = extract_info_string(bytes, b"/Title");
    let author = extract_info_string(bytes, b"/Author");

    // 1. Scan all stream objects (`<< ... >> stream ... endstream`) sequentially
    let raw_streams = scan_pdf_streams(bytes);

    // 2. Harvest `/ToUnicode` CMap tables first so content-stream hex codes decode to real Unicode
    let mut cmap: HashMap<u32, String> = HashMap::new();
    let mut content_streams: Vec<Vec<u8>> = Vec::new();

    for (dict_slice, stream_bytes) in raw_streams {
        let decoded = decode_pdf_stream(dict_slice, stream_bytes);
        if decoded.is_empty() {
            continue;
        }
        if contains_subslice(&decoded, b"beginbfchar")
            || contains_subslice(&decoded, b"beginbfrange")
        {
            parse_tounicode_cmap(&decoded, &mut cmap);
        } else if !is_non_text_pdf_stream(dict_slice) {
            content_streams.push(decoded);
        }
    }

    // 3. Execute PostScript text-operator FSM on each content stream
    let mut all_spans: Vec<PositionedSpan> = Vec::new();
    let mut page_idx = 0usize;
    for stream in &content_streams {
        let mut page_spans = Vec::new();
        run_text_operator_fsm(stream, page_idx, &cmap, &mut page_spans);
        if !page_spans.is_empty() {
            all_spans.extend(page_spans);
            page_idx += 1;
        }
    }

    // 4. Format clustered 2D reading-order Markdown
    let mut md = String::new();
    if let Some(ref t) = title {
        md.push_str(&format!("# {}\n\n", t));
        if let Some(ref a) = author {
            md.push_str(&format!("*Author: {}*\n\n", a));
        }
    }

    if all_spans.is_empty() {
        let label = source_name.unwrap_or("document.pdf");
        md.push_str(&format!(
            "# PDF Document: `{}`\n\n\
             - **Size:** {} bytes ({:.1} KB)\n\
             - **Extractable Text Streams:** 0\n\n\
             *This PDF appears to be encrypted, vector-outlined, or a scanned image-only PDF without embedded text streams.*",
            label,
            bytes.len(),
            bytes.len() as f64 / 1024.0
        ));
        return PdfExtraction {
            title,
            markdown: md,
        };
    }

    // Compute robust document typography statistics (median + MAD + upper-cluster centroid)
    // to separate headings from body text adaptively across arbitrary font scales.
    let mut sizes: Vec<f64> = all_spans
        .iter()
        .map(|s| s.font_size)
        .filter(|&sz| sz > 0.5)
        .collect();
    sizes.sort_by(|a, b| a.total_cmp(b));
    let median_font = sizes.get(sizes.len() / 2).copied().unwrap_or(11.0);
    let mut abs_devs: Vec<f64> = sizes.iter().map(|&sz| (sz - median_font).abs()).collect();
    abs_devs.sort_by(|a, b| a.total_cmp(b));
    let mad_font = abs_devs.get(abs_devs.len() / 2).copied().unwrap_or(0.0);
    // Robust upper-tail threshold: median + 2.5*MAD, with a Weber-fraction floor (18%) when
    // body text accounts for >50% of spans (causing MAD == 0).
    let heading_floor = (median_font + 2.5 * mad_font).max(median_font * 1.18);
    // Split detected heading tiers cleanly at the midpoint of the upper font-size cluster.
    let upper_sizes: Vec<f64> = sizes
        .iter()
        .copied()
        .filter(|&sz| sz >= heading_floor)
        .collect();
    let major_heading_floor = if upper_sizes.is_empty() {
        heading_floor * 1.25
    } else {
        let upper_min = upper_sizes[0];
        let upper_max = *upper_sizes.last().unwrap_or(&upper_min);
        if (upper_max - upper_min) > 1.0 {
            0.5 * (upper_min + upper_max)
        } else {
            upper_min
        }
    };

    let total_pages = page_idx.max(1);
    for p in 0..total_pages {
        let mut spans: Vec<&PositionedSpan> =
            all_spans.iter().filter(|s| s.page_idx == p).collect();
        if spans.is_empty() {
            continue;
        }
        if total_pages > 1 {
            md.push_str(&format!("## Page {}\n\n", p + 1));
        }

        // Sort top-to-bottom (descending Y in PDF coordinate space), then left-to-right (ascending X)
        spans.sort_by(|a, b| b.y.total_cmp(&a.y).then_with(|| a.x.total_cmp(&b.x)));

        // Cluster spans into horizontal text lines using adaptive vertical tolerance (0.45 * font_size)
        let mut lines: Vec<(f64, f64, String)> = Vec::new();
        for sp in spans {
            let y_tol = (sp.font_size * 0.45).clamp(2.5, 14.0);
            if let Some(last) = lines.last_mut() {
                if (last.0 - sp.y).abs() <= y_tol {
                    if !last.2.ends_with(' ') && !sp.text.starts_with(' ') {
                        last.2.push(' ');
                    }
                    last.2.push_str(&sp.text);
                    last.1 = last.1.max(sp.font_size);
                    continue;
                }
            }
            lines.push((sp.y, sp.font_size, sp.text.clone()));
        }

        // Compute adaptive per-page baseline line spacing (`median_dy`) for paragraph break detection
        let mut line_gaps: Vec<f64> = lines
            .windows(2)
            .map(|w| (w[0].0 - w[1].0).abs())
            .filter(|&dy| dy > 0.5)
            .collect();
        line_gaps.sort_by(|a, b| a.total_cmp(b));
        let median_dy = line_gaps.get(line_gaps.len() / 2).copied();

        // Compute adaptive line-length cap for headings from the page's median line length
        let mut line_lens: Vec<usize> = lines.iter().map(|(_, _, t)| t.trim().len()).collect();
        line_lens.sort_unstable();
        let median_line_len = line_lens.get(line_lens.len() / 2).copied().unwrap_or(60);
        let heading_len_cap = (median_line_len * 2).clamp(80, 180);

        // Group lines into paragraphs and font-size headings
        let mut prev_y: Option<f64> = None;
        let mut paragraph = String::new();

        let flush_para = |md: &mut String, paragraph: &mut String| {
            let clean = paragraph.split_whitespace().collect::<Vec<_>>().join(" ");
            if !clean.is_empty() {
                md.push_str(&clean);
                md.push_str("\n\n");
            }
            paragraph.clear();
        };

        for (y, fsize, text) in lines {
            let clean_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if clean_line.is_empty() {
                continue;
            }
            let is_heading = fsize >= heading_floor
                && clean_line.len() <= heading_len_cap
                && !clean_line.ends_with('.');
            let gap_threshold = median_dy
                .map(|dy| dy * 1.35)
                .unwrap_or_else(|| fsize.max(median_font) * 1.5);
            let large_vertical_gap = prev_y.is_some_and(|py| (py - y).abs() > gap_threshold);

            if is_heading {
                flush_para(&mut md, &mut paragraph);
                let prefix = if fsize >= major_heading_floor && total_pages == 1 {
                    "##"
                } else {
                    "###"
                };
                md.push_str(&format!("{} {}\n\n", prefix, clean_line));
            } else {
                if large_vertical_gap {
                    flush_para(&mut md, &mut paragraph);
                }
                if !paragraph.is_empty() {
                    if paragraph.ends_with('-') {
                        paragraph.pop();
                    } else {
                        paragraph.push(' ');
                    }
                }
                paragraph.push_str(&clean_line);
            }
            prev_y = Some(y);
        }
        flush_para(&mut md, &mut paragraph);
    }

    PdfExtraction {
        title,
        markdown: md.trim_end().to_string(),
    }
}

/// Tokenizes PDF dictionary `/Key /Value` name pairs cleanly regardless of whitespace formatting.
fn parse_pdf_dict_keys(dict: &[u8]) -> Vec<(&[u8], Option<&[u8]>)> {
    let is_delim = |b: u8| {
        b.is_ascii_whitespace()
            || matches!(
                b,
                b'/' | b'<' | b'>' | b'[' | b']' | b'(' | b')' | b'{' | b'}' | b'%'
            )
    };
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < dict.len() {
        if dict[i] == b'/' {
            let key_start = i + 1;
            let mut key_end = key_start;
            while key_end < dict.len() && !is_delim(dict[key_end]) {
                key_end += 1;
            }
            let key = &dict[key_start..key_end];
            let mut j = key_end;
            while j < dict.len() && dict[j].is_ascii_whitespace() {
                j += 1;
            }
            let val = if j < dict.len() && dict[j] == b'/' {
                let v_start = j + 1;
                let mut v_end = v_start;
                while v_end < dict.len() && !is_delim(dict[v_end]) {
                    v_end += 1;
                }
                i = v_end;
                Some(&dict[v_start..v_end])
            } else {
                i = key_end;
                None
            };
            if !key.is_empty() {
                pairs.push((key, val));
            }
        } else {
            i += 1;
        }
    }
    pairs
}

fn is_non_text_pdf_stream(dict: &[u8]) -> bool {
    parse_pdf_dict_keys(dict).into_iter().any(|(k, v)| {
        (k == b"Subtype" && matches!(v, Some(b"Image" | b"Type1C" | b"CIDFontType0C")))
            || (k == b"Type" && matches!(v, Some(b"XRef" | b"ObjStm")))
            || k == b"Length1"
    })
}

fn scan_pdf_streams(bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut streams = Vec::new();
    let mut pos = 0usize;
    let needle = b"stream";
    let end_needle = b"endstream";

    while let Some(rel) = find_subslice(&bytes[pos..], needle) {
        let s_pos = pos + rel;
        // Require `stream` not to be part of `endstream` or another identifier
        let prev_ok =
            s_pos == 0 || bytes[s_pos - 1].is_ascii_whitespace() || bytes[s_pos - 1] == b'>';
        let after = s_pos + needle.len();
        let eol_len = if bytes.get(after..after + 2) == Some(b"\r\n") {
            2
        } else if bytes.get(after) == Some(&b'\n') || bytes.get(after) == Some(&b'\r') {
            1
        } else {
            0
        };

        if prev_ok && eol_len > 0 {
            let payload_start = after + eol_len;
            if let Some(end_rel) = find_subslice(&bytes[payload_start..], end_needle) {
                let mut payload_end = payload_start + end_rel;
                while payload_end > payload_start && matches!(bytes[payload_end - 1], b'\r' | b'\n')
                {
                    payload_end -= 1;
                }
                let dict_start = s_pos.saturating_sub(1024);
                let dict_slice = find_last_subslice(&bytes[dict_start..s_pos], b"<<")
                    .map(|off| &bytes[dict_start + off..s_pos])
                    .unwrap_or(&bytes[dict_start..s_pos]);
                streams.push((dict_slice, &bytes[payload_start..payload_end]));
                pos = payload_start + end_rel + end_needle.len();
                continue;
            }
        }
        pos = after;
    }

    streams
}

fn decode_pdf_stream(dict: &[u8], raw: &[u8]) -> Vec<u8> {
    let mut data = raw.to_vec();
    if contains_subslice(dict, b"/ASCIIHexDecode") {
        data = decode_ascii_hex(&data);
    }
    if contains_subslice(dict, b"/ASCII85Decode") {
        data = decode_ascii85(&data);
    }
    if contains_subslice(dict, b"/FlateDecode") {
        // Trim leading whitespace occasionally emitted before zlib header
        let start = data
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(0);
        if let Ok(inflated) = inflate_zlib(&data[start..], DEFAULT_MAX_INFLATE_BYTES) {
            return inflated;
        }
        return Vec::new();
    }
    // Skip unsupported binary image filters (DCTDecode/JPXDecode/CCITTFaxDecode/LZWDecode)
    if contains_subslice(dict, b"/DCTDecode")
        || contains_subslice(dict, b"/JPXDecode")
        || contains_subslice(dict, b"/CCITTFaxDecode")
    {
        return Vec::new();
    }
    data
}

fn decode_ascii_hex(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 2);
    let mut hi: Option<u8> = None;
    for &b in input {
        if b == b'>' {
            break;
        }
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        if let Some(h) = hi.take() {
            out.push((h << 4) | digit);
        } else {
            hi = Some(digit);
        }
    }
    if let Some(h) = hi {
        out.push(h << 4);
    }
    out
}

fn decode_ascii85(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut group = [0u32; 5];
    let mut count = 0usize;
    let mut i = 0usize;
    while i < input.len() {
        let b = input[i];
        if b == b'~' && input.get(i + 1) == Some(&b'>') {
            break;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'z' && count == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            i += 1;
            continue;
        }
        if (b'!'..=b'u').contains(&b) {
            group[count] = u32::from(b - b'!');
            count += 1;
            if count == 5 {
                let mut val: u64 = 0;
                for &g in &group {
                    val = val * 85 + u64::from(g);
                }
                let bytes = (val as u32).to_be_bytes();
                out.extend_from_slice(&bytes);
                count = 0;
            }
        }
        i += 1;
    }
    if count >= 2 {
        for slot in group.iter_mut().skip(count) {
            *slot = 84;
        }
        let mut val: u64 = 0;
        for &g in &group {
            val = val * 85 + u64::from(g);
        }
        let bytes = (val as u32).to_be_bytes();
        out.extend_from_slice(&bytes[..count - 1]);
    }
    out
}

fn parse_tounicode_cmap(data: &[u8], cmap: &mut HashMap<u32, String>) {
    let text = String::from_utf8_lossy(data);
    let mut mode = 0u8; // 1 = bfchar, 2 = bfrange
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with("beginbfchar") {
            mode = 1;
            continue;
        }
        if trimmed.ends_with("beginbfrange") {
            mode = 2;
            continue;
        }
        if trimmed.starts_with("endbfchar") || trimmed.starts_with("endbfrange") {
            mode = 0;
            continue;
        }
        let hex_tokens = extract_hex_angle_tokens(trimmed);
        if mode == 1 && hex_tokens.len() >= 2 {
            if let Ok(src) = u32::from_str_radix(hex_tokens[0], 16) {
                let dst = decode_utf16be_hex(hex_tokens[1]);
                if !dst.is_empty() {
                    cmap.insert(src, dst);
                }
            }
        } else if mode == 2 && hex_tokens.len() >= 3 {
            if let (Ok(start), Ok(end)) = (
                u32::from_str_radix(hex_tokens[0], 16),
                u32::from_str_radix(hex_tokens[1], 16),
            ) {
                if end >= start && (end - start) <= 512 {
                    if let Ok(mut dst_code) = u32::from_str_radix(hex_tokens[2], 16) {
                        for code in start..=end {
                            if let Some(ch) = char::from_u32(dst_code) {
                                cmap.insert(code, ch.to_string());
                            }
                            dst_code = dst_code.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
}

fn extract_hex_angle_tokens(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            break;
        };
        out.push(after[..close].trim());
        rest = &after[close + 1..];
    }
    out
}

fn decode_utf16be_hex(hex: &str) -> String {
    let raw = decode_ascii_hex(hex.as_bytes());
    if raw.len() >= 2 && raw.len().is_multiple_of(2) {
        let u16s: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_be_bytes(c))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    raw.into_iter().map(char::from).collect()
}

fn extract_info_string(bytes: &[u8], key: &[u8]) -> Option<String> {
    let pos = find_subslice(bytes, key)?;
    let mut cursor = pos + key.len();
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'(') {
        let (lit, _) = parse_pdf_literal_string(bytes, cursor);
        let decoded = decode_pdf_text_bytes(&lit, &HashMap::new());
        let clean = decoded.trim().to_string();
        return (!clean.is_empty()).then_some(clean);
    }
    if bytes.get(cursor) == Some(&b'<') && bytes.get(cursor + 1) != Some(&b'<') {
        let (hex_bytes, _) = parse_pdf_hex_string(bytes, cursor);
        let decoded = decode_pdf_text_bytes(&hex_bytes, &HashMap::new());
        let clean = decoded.trim().to_string();
        return (!clean.is_empty()).then_some(clean);
    }
    None
}

// ============================================================================
// PostScript PDF Content-Stream Tokenizer & Text-Operator State Machine
// ============================================================================

#[derive(Debug, Clone)]
enum PdfToken {
    Number(f64),
    StringBytes(Vec<u8>),
    Array(Vec<PdfToken>),
    Operator(String),
}

fn tokenize_content_stream(stream: &[u8]) -> Vec<PdfToken> {
    let mut tokens = Vec::new();
    let len = stream.len();
    let mut i = 0usize;

    while i < len {
        let b = stream[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Comment `% ... \n`
        if b == b'%' {
            while i < len && stream[i] != b'\n' && stream[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        // Literal string `(...)`
        if b == b'(' {
            let (s, next) = parse_pdf_literal_string(stream, i);
            tokens.push(PdfToken::StringBytes(s));
            i = next;
            continue;
        }
        // Hex string `<...>` (skip dictionary delimiters `<<` / `>>`)
        if b == b'<' {
            if stream.get(i + 1) == Some(&b'<') {
                i += 2;
                continue;
            }
            let (s, next) = parse_pdf_hex_string(stream, i);
            tokens.push(PdfToken::StringBytes(s));
            i = next;
            continue;
        }
        if b == b'>' && stream.get(i + 1) == Some(&b'>') {
            i += 2;
            continue;
        }
        // Array `[ ... ]` (used by `TJ`)
        if b == b'[' {
            i += 1;
            let mut items = Vec::new();
            while i < len {
                let c = stream[i];
                if c.is_ascii_whitespace() {
                    i += 1;
                } else if c == b']' {
                    i += 1;
                    break;
                } else if c == b'(' {
                    let (s, next) = parse_pdf_literal_string(stream, i);
                    items.push(PdfToken::StringBytes(s));
                    i = next;
                } else if c == b'<' {
                    let (s, next) = parse_pdf_hex_string(stream, i);
                    items.push(PdfToken::StringBytes(s));
                    i = next;
                } else {
                    let start = i;
                    while i < len
                        && !stream[i].is_ascii_whitespace()
                        && !matches!(stream[i], b']' | b'(' | b'<' | b'/')
                    {
                        i += 1;
                    }
                    if let Ok(s) = std::str::from_utf8(&stream[start..i]) {
                        if let Ok(num) = s.parse::<f64>() {
                            items.push(PdfToken::Number(num));
                        }
                    }
                    if i == start {
                        i += 1;
                    }
                }
            }
            tokens.push(PdfToken::Array(items));
            continue;
        }
        // Name `/FontName`
        if b == b'/' {
            let start = i;
            i += 1;
            while i < len
                && !stream[i].is_ascii_whitespace()
                && !matches!(stream[i], b'/' | b'[' | b']' | b'(' | b')' | b'<' | b'>')
            {
                i += 1;
            }
            let name = String::from_utf8_lossy(&stream[start..i]).into_owned();
            tokens.push(PdfToken::Operator(name));
            continue;
        }
        // Number or Operator word
        let start = i;
        while i < len
            && !stream[i].is_ascii_whitespace()
            && !matches!(stream[i], b'/' | b'[' | b']' | b'(' | b')' | b'<' | b'>')
        {
            i += 1;
        }
        if let Ok(word) = std::str::from_utf8(&stream[start..i]) {
            if let Ok(num) = word.parse::<f64>() {
                tokens.push(PdfToken::Number(num));
            } else {
                tokens.push(PdfToken::Operator(word.to_string()));
            }
        }
        if i == start {
            i += 1;
        }
    }

    tokens
}

fn parse_pdf_literal_string(stream: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut i = start + 1;
    let mut depth = 1usize;

    while i < stream.len() && depth > 0 {
        let b = stream[i];
        if b == b'\\' && i + 1 < stream.len() {
            i += 1;
            let esc = stream[i];
            match esc {
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0C),
                b'(' | b')' | b'\\' => out.push(esc),
                b'\r' => {
                    if stream.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                }
                b'\n' => {}
                b'0'..=b'7' => {
                    let mut oct = u32::from(esc - b'0');
                    for _ in 0..2 {
                        if let Some(&next_b) = stream.get(i + 1) {
                            if (b'0'..=b'7').contains(&next_b) {
                                oct = (oct << 3) | u32::from(next_b - b'0');
                                i += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    out.push((oct & 0xFF) as u8);
                }
                other => out.push(other),
            }
            i += 1;
        } else if b == b'(' {
            depth += 1;
            out.push(b);
            i += 1;
        } else if b == b')' {
            depth -= 1;
            if depth > 0 {
                out.push(b);
            }
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    (out, i)
}

fn parse_pdf_hex_string(stream: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut i = start + 1;
    let payload_start = i;
    while i < stream.len() && stream[i] != b'>' {
        i += 1;
    }
    let decoded = decode_ascii_hex(&stream[payload_start..i]);
    if i < stream.len() {
        i += 1;
    }
    (decoded, i)
}

fn decode_pdf_text_bytes(raw: &[u8], cmap: &HashMap<u32, String>) -> String {
    // Handle UTF-16BE BOM (`FE FF`)
    if raw.len() >= 2 && raw[0] == 0xFE && raw[1] == 0xFF {
        let u16s: Vec<u16> = raw[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_be_bytes(c))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    // Try 2-byte CMap lookup first when all byte pairs resolve in `/ToUnicode`
    if !cmap.is_empty() && raw.len() >= 2 && raw.len().is_multiple_of(2) {
        let mut mapped = String::new();
        let mut hits = 0usize;
        for &pair in raw.as_chunks::<2>().0 {
            let code = u32::from(u16::from_be_bytes(pair));
            if let Some(s) = cmap.get(&code) {
                mapped.push_str(s);
                hits += 1;
            }
        }
        if hits * 2 >= raw.len() / 2 && !mapped.is_empty() {
            return mapped;
        }
    }
    // Single-byte CMap + WinAnsi/Latin-1 decoding
    let mut out = String::with_capacity(raw.len());
    for &b in raw {
        if let Some(s) = cmap.get(&u32::from(b)) {
            out.push_str(s);
        } else if (0x20..=0x7E).contains(&b) || b == b'\t' || b == b'\n' || b >= 0xA0 {
            out.push(char::from(b));
        }
    }
    out
}

fn run_text_operator_fsm(
    stream: &[u8],
    page_idx: usize,
    cmap: &HashMap<u32, String>,
    out_spans: &mut Vec<PositionedSpan>,
) {
    let tokens = tokenize_content_stream(stream);
    let mut operands: Vec<PdfToken> = Vec::new();
    let mut in_text = false;
    let mut cur_x = 0.0f64;
    let mut cur_y = 0.0f64;
    let mut font_size = 11.0f64;
    let mut leading = 14.0f64;
    let mut line_seq = 0usize;

    for tok in tokens {
        match tok {
            PdfToken::Operator(ref op) if !op.starts_with('/') => {
                match op.as_str() {
                    "BT" => {
                        in_text = true;
                        cur_x = 0.0;
                        cur_y = -(line_seq as f64) * 14.0;
                    }
                    "ET" => {
                        in_text = false;
                    }
                    "Tf" => {
                        if let Some(PdfToken::Number(sz)) = operands.last() {
                            if sz.abs() > 0.1 {
                                font_size = sz.abs();
                            }
                        }
                    }
                    "TL" => {
                        if let Some(PdfToken::Number(tl)) = operands.last() {
                            leading = tl.abs().max(4.0);
                        }
                    }
                    "Td" | "TD" if in_text => {
                        let nums: Vec<f64> = operands
                            .iter()
                            .filter_map(|t| match t {
                                PdfToken::Number(n) => Some(*n),
                                _ => None,
                            })
                            .collect();
                        if nums.len() >= 2 {
                            let tx = nums[nums.len() - 2];
                            let ty = nums[nums.len() - 1];
                            cur_x += tx;
                            cur_y += ty;
                            if op == "TD" {
                                leading = ty.abs().max(4.0);
                            }
                        }
                    }
                    "Tm" if in_text => {
                        let nums: Vec<f64> = operands
                            .iter()
                            .filter_map(|t| match t {
                                PdfToken::Number(n) => Some(*n),
                                _ => None,
                            })
                            .collect();
                        if nums.len() >= 6 {
                            let a = nums[nums.len() - 6];
                            let d = nums[nums.len() - 3];
                            let e = nums[nums.len() - 2];
                            let f = nums[nums.len() - 1];
                            let scale = a.abs().max(d.abs());
                            if scale > 1.0 && font_size <= 2.0 {
                                font_size *= scale;
                            }
                            cur_x = e;
                            cur_y = f;
                        }
                    }
                    "T*" if in_text => {
                        cur_y -= leading;
                    }
                    "Tj" | "'" | "\"" if in_text => {
                        if op == "'" || op == "\"" {
                            cur_y -= leading;
                        }
                        if let Some(PdfToken::StringBytes(raw)) = operands.last() {
                            let text = decode_pdf_text_bytes(raw, cmap);
                            if !text.trim().is_empty() {
                                out_spans.push(PositionedSpan {
                                    page_idx,
                                    x: cur_x,
                                    y: cur_y,
                                    font_size,
                                    text,
                                });
                                line_seq += 1;
                            }
                        }
                    }
                    "TJ" if in_text => {
                        if let Some(PdfToken::Array(arr)) = operands.last() {
                            let mut buf = String::new();
                            for item in arr {
                                match item {
                                    PdfToken::StringBytes(raw) => {
                                        buf.push_str(&decode_pdf_text_bytes(raw, cmap));
                                    }
                                    PdfToken::Number(kern) if *kern <= -140.0 => {
                                        if !buf.ends_with(' ') {
                                            buf.push(' ');
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            if !buf.trim().is_empty() {
                                out_spans.push(PositionedSpan {
                                    page_idx,
                                    x: cur_x,
                                    y: cur_y,
                                    font_size,
                                    text: buf,
                                });
                                line_seq += 1;
                            }
                        }
                    }
                    _ => {}
                }
                operands.clear();
            }
            other => {
                operands.push(other);
                if operands.len() > 32 {
                    operands.remove(0);
                }
            }
        }
    }
}

#[inline]
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[inline]
fn find_last_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

#[inline]
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    find_subslice(haystack, needle).is_some()
}

#[cfg(test)]
mod tests {
    use crate::context::doc_convert::{ConversionPolicy, DocumentConverterRegistry};

    #[test]
    fn test_pdf_converter_extracts_headings_paragraphs_and_tj_kerning() {
        let minimal_pdf = b"%PDF-1.4\n\
1 0 obj << /Title (Design RFC) /Author (Aswin) >> endobj\n\
2 0 obj << /Length 168 >>\n\
stream\n\
BT\n\
/F1 20 Tf\n\
72 720 Td\n\
(System Overview) Tj\n\
/F1 11 Tf\n\
0 -28 Td\n\
[(Pure ) -180 (Rust ) -180 (PDF ) -180 (Extraction)] TJ\n\
0 -14 Td\n\
(Works without external crates.) Tj\n\
ET\n\
endstream\n\
endobj\n\
%%EOF\n";

        let doc = DocumentConverterRegistry::global()
            .convert_bytes(
                minimal_pdf,
                Some("rfc.pdf"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .expect("PDF must convert");
        assert_eq!(doc.converter_name, "pdf");
        assert_eq!(doc.title.as_deref(), Some("Design RFC"));
        assert!(doc.markdown.contains("# Design RFC"));
        assert!(doc.markdown.contains("## System Overview"));
        assert!(doc.markdown.contains("Pure Rust PDF Extraction"));
        assert!(doc.markdown.contains("Works without external crates."));
    }

    #[test]
    fn test_pdf_converter_decompresses_flatedecode_zlib_stream() {
        let flate_stream: &[u8] = &[
            0x78, 0x9C, 0x73, 0x0A, 0x51, 0xD0, 0x77, 0x33, 0x54, 0x30, 0x34, 0x52, 0x08, 0x49,
            0x53, 0x30, 0x37, 0x52, 0x30, 0x37, 0x30, 0x50, 0x08, 0x49, 0x51, 0xD0, 0x70, 0x49,
            0x4D, 0xCE, 0xCF, 0x2D, 0x28, 0x4A, 0x2D, 0x2E, 0x4E, 0x4D, 0x51, 0x70, 0xCB, 0x49,
            0x2C, 0x49, 0x05, 0x89, 0xA4, 0xA4, 0x2A, 0x04, 0xB8, 0xB8, 0x29, 0x94, 0xA4, 0x56,
            0x94, 0x28, 0x6A, 0x2A, 0x84, 0x64, 0x29, 0xB8, 0x86, 0x00, 0x00, 0x4B, 0x91, 0x13,
            0x08,
        ];
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.5\n1 0 obj << /Filter /FlateDecode /Length 71 >>\nstream\n");
        pdf.extend_from_slice(flate_stream);
        pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");

        let doc = DocumentConverterRegistry::global()
            .convert_bytes(
                &pdf,
                Some("compressed.pdf"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .expect("FlateDecode PDF must convert");
        assert!(doc.markdown.contains("Decompressed FlateDecode PDF text!"));
    }
}
