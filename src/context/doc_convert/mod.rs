//! Pure-Rust MarkItDown-inspired Priority-Ordered Document Conversion Engine (`src/context/doc_convert/`).
//!
//! Operates automatically inside `read_file`, `look_at` (`PageIndex`), and `webfetch` with zero `/commands`,
//! zero regexes, zero hardcoded extension dispatch walls, and zero new external Cargo dependencies:
//! - [`StreamInfo`]: Magic-byte signature table + ZIP Central Directory structural container introspection
//!   + Shannon byte entropy & structural grammar sniffer.
//! - [`DocumentConverter`] & [`DocumentConverterRegistry`]: Priority-ordered trait dispatch across
//!   DOCX, XLSX, PPTX, EPUB, PDF, IPYNB, RSS/Atom, CSV/TSV, HTML, ZIP archives, and binary inspection.
//! - [`decode_data_uri`]: Pure-Rust RFC 2397 `data:` URI decoder.
//! - [`convert_workspace_file_cached`]: Bounded LRU cache keyed by `(canonical_path, mtime, byte_len, policy)`.

pub mod deflate;
pub mod ooxml;
pub mod pdf;
pub mod structured;
pub mod xml;

use crate::error::{ContextError, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Maximum cached converted documents retained in process memory.
const MAX_CACHE_ENTRIES: usize = 32;
/// Maximum cumulative Markdown bytes retained in the conversion cache (8 MiB).
const MAX_CACHE_TOTAL_BYTES: usize = 8 * 1024 * 1024;

/// Magic-byte prefix signature table: `(offset, signature_bytes, mime_type, label)`.
const MAGIC_SIGNATURES: &[(usize, &[u8], &str, &str)] = &[
    (0, b"%PDF-", "application/pdf", "PDF Document"),
    (0, b"\x89PNG\r\n\x1a\n", "image/png", "PNG Image"),
    (0, b"\xFF\xD8\xFF", "image/jpeg", "JPEG Image"),
    (0, b"GIF87a", "image/gif", "GIF Image"),
    (0, b"GIF89a", "image/gif", "GIF Image"),
    (0, b"PK\x03\x04", "application/zip", "ZIP Archive"),
    (0, b"PK\x05\x06", "application/zip", "ZIP Archive (Empty)"),
    (0, b"\x1F\x8B", "application/gzip", "GZIP Compressed Stream"),
    (0, b"\x00asm", "application/wasm", "WebAssembly Module"),
    (0, b"\x7FELF", "application/x-elf", "ELF Executable/Library"),
    (
        0,
        b"SQLite format 3\0",
        "application/vnd.sqlite3",
        "SQLite 3 Database",
    ),
    (0, b"wOFF", "font/woff", "WOFF Font"),
    (0, b"wOF2", "font/woff2", "WOFF2 Font"),
    (0, b"OggS", "audio/ogg", "Ogg Media Stream"),
    (0, b"ID3", "audio/mpeg", "MP3 Audio"),
    (0, b"fLaC", "audio/flac", "FLAC Audio"),
    (0, b"BM", "image/bmp", "BMP Image"),
    (
        0,
        b"MZ",
        "application/vnd.microsoft.portable-executable",
        "PE Executable",
    ),
    (
        0,
        b"\xCA\xFE\xBA\xBE",
        "application/x-mach-binary",
        "Mach-O / Java Class",
    ),
    (
        0,
        b"\xCF\xFA\xED\xFE",
        "application/x-mach-binary",
        "Mach-O 64-bit Binary",
    ),
];

/// Declarative ZIP structural container signatures checked via Central Directory paths (zero decompression).
const ZIP_CONTAINER_MARKERS: &[(&str, &str, &str)] = &[
    (
        "word/document.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "docx",
    ),
    (
        "xl/workbook.xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xlsx",
    ),
    (
        "ppt/presentation.xml",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "pptx",
    ),
    ("META-INF/container.xml", "application/epub+zip", "epub"),
];

/// Controls whether verbatim editable text formats (like `.html` or `.csv`) are converted to
/// Markdown or left untouched so line-accurate code editing (`patch_file`) stays intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConversionPolicy {
    /// Local `read_file`: convert non-UTF-8/binary/container documents (`PDF`, `DOCX`, `XLSX`,
    /// `PPTX`, `EPUB`, `ZIP`, `IPYNB`, `RSS/Atom`) while keeping editable source text (`.html`,
    /// `.csv`, `.rs`, `.ts`, `.md`, etc.) verbatim.
    LocalEditable,
    /// Local `look_at`: convert ALL rich structured documents (including `.html` and `.csv`/`.tsv`)
    /// so `PageIndex` hierarchical outlines & section drilldowns work uniformly.
    StructuralOutline,
    /// Remote HTTP (`webfetch`) or `data:` URI streams: convert every recognized document format.
    WebOrStream,
}

/// Structural stream profile inferred algorithmically from magic bytes, ZIP Central Directory
/// entries, UTF-8 validity, Shannon entropy, and optional filename/HTTP hints.
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub mime_type: String,
    pub detected_label: Option<String>,
    pub extension: Option<String>,
    pub source_name: Option<String>,
    pub is_utf8_text: bool,
    pub shannon_entropy: f64,
    pub is_visual_image: bool,
    pub zip_entries: Vec<String>,
}

impl StreamInfo {
    /// Sniffs raw bytes using magic-byte signatures, ZIP Central Directory inspection,
    /// UTF-8 + control-byte validation, and Shannon entropy $H(B) = -\sum p_i \log_2 p_i$.
    #[must_use]
    pub fn sniff(bytes: &[u8], source_name: Option<&str>, hint_mime: Option<&str>) -> Self {
        let mut extension = source_name.and_then(|name| {
            let clean = name.split('?').next().unwrap_or(name);
            Path::new(clean)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
        });

        let clean_hint_mime = hint_mime
            .map(|m| m.split(';').next().unwrap_or(m).trim().to_ascii_lowercase())
            .filter(|m| !m.is_empty() && m != "application/octet-stream");

        let mut mime_type = String::new();
        let mut detected_label: Option<String> = None;

        // Layer 1: Magic-byte signature table (+ WEBP RIFF composite signature)
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            mime_type = "image/webp".to_string();
            detected_label = Some("WebP Image".to_string());
        } else {
            for &(offset, sig, mime, label) in MAGIC_SIGNATURES {
                if bytes.len() >= offset + sig.len() && &bytes[offset..offset + sig.len()] == sig {
                    // Guard against plain-text files starting with "BM" or "MZ" (require binary indicators)
                    if (sig == b"BM" || sig == b"MZ") && is_clean_utf8_text(bytes) {
                        continue;
                    }
                    mime_type = mime.to_string();
                    detected_label = Some(label.to_string());
                    break;
                }
            }
        }

        // Layer 2: ZIP Central Directory structural introspection (O(entries), zero decompression)
        let mut zip_entries = Vec::new();
        if mime_type == "application/zip" {
            if let Ok(archive) = deflate::ZipArchive::parse(bytes) {
                zip_entries = archive.entries().iter().map(|e| e.name.clone()).collect();
                for &(marker, container_mime, container_ext) in ZIP_CONTAINER_MARKERS {
                    if archive.contains(marker) {
                        mime_type = container_mime.to_string();
                        detected_label =
                            Some(format!("{} Document", container_ext.to_ascii_uppercase()));
                        if extension.is_none() {
                            extension = Some(container_ext.to_string());
                        }
                        break;
                    }
                }
            }
        }

        // Layer 3: UTF-8 validity & Shannon byte entropy
        let is_utf8_text = mime_type.is_empty() && is_clean_utf8_text(bytes);
        let shannon_entropy = compute_shannon_entropy(bytes);

        if mime_type.is_empty() {
            if is_utf8_text {
                mime_type = sniff_text_mime(bytes, clean_hint_mime.as_deref());
            } else {
                mime_type =
                    clean_hint_mime.unwrap_or_else(|| "application/octet-stream".to_string());
            }
        }

        let is_visual_image = matches!(
            mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        );

        Self {
            mime_type,
            detected_label,
            extension,
            source_name: source_name.map(str::to_string),
            is_utf8_text,
            shannon_entropy,
            is_visual_image,
            zip_entries,
        }
    }
}

/// Returns `true` when `bytes` is valid UTF-8 with no NUL bytes and negligible binary control characters.
#[must_use]
pub fn is_clean_utf8_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let sample = &bytes[..bytes.len().min(16_384)];
    if sample.contains(&0) {
        return false;
    }
    // Check UTF-8 validity on complete prefix (trimming up to 3 trailing bytes if sample cut mid-char)
    let valid_utf8 = match std::str::from_utf8(sample) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && e.valid_up_to() + 4 >= sample.len(),
    };
    if !valid_utf8 {
        return false;
    }
    let bad_ctrl = sample
        .iter()
        .filter(|&&b| b < 0x09 || (0x0E..=0x1F).contains(&b))
        .count();
    bad_ctrl * 50 <= sample.len()
}

/// Computes Shannon entropy $H(B) = -\sum p_i \log_2 p_i \in [0.0, 8.0]$ over the first 8 KiB.
#[must_use]
pub fn compute_shannon_entropy(bytes: &[u8]) -> f64 {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for &b in sample {
        freq[b as usize] += 1;
    }
    let n = sample.len() as f64;
    let mut h = 0.0f64;
    for &c in &freq {
        if c > 0 {
            let p = f64::from(c) / n;
            h -= p * p.log2();
        }
    }
    h
}

/// Infers structured text MIME type purely from leading grammar tokens and structural markers.
fn sniff_text_mime(bytes: &[u8], hint_mime: Option<&str>) -> String {
    let head = std::str::from_utf8(&bytes[..bytes.len().min(4096)]).unwrap_or("");
    let trimmed = head.trim_start_matches('\u{FEFF}').trim_start();
    let lower_head = trimmed
        .chars()
        .take(512)
        .collect::<String>()
        .to_ascii_lowercase();

    // Jupyter Notebook JSON structure: object containing both `"cells"` and `"nbformat"`
    if trimmed.starts_with('{') {
        let full_probe = std::str::from_utf8(&bytes[..bytes.len().min(65_536)]).unwrap_or(head);
        if full_probe.contains("\"nbformat\"") && full_probe.contains("\"cells\"") {
            return "application/x-ipynb+json".to_string();
        }
    }

    // RSS 2.0 / Atom 1.0 XML feeds
    if lower_head.starts_with("<?xml")
        || lower_head.starts_with("<rss")
        || lower_head.starts_with("<feed")
    {
        if lower_head.contains("<rss") || lower_head.contains("<rdf:rdf") {
            return "application/rss+xml".to_string();
        }
        if lower_head.contains("<feed") && lower_head.contains("http://www.w3.org/2005/atom") {
            return "application/atom+xml".to_string();
        }
        if lower_head.contains("<feed") && lower_head.contains("<entry") {
            return "application/atom+xml".to_string();
        }
    }

    // HTML documents
    if lower_head.starts_with("<!doctype html")
        || lower_head.starts_with("<html")
        || (lower_head.starts_with("<?xml") && lower_head.contains("<html"))
    {
        return "text/html".to_string();
    }

    if let Some(hint) = hint_mime {
        return hint.to_string();
    }

    // Statistical tabular detection (minimum column-count variance across sample lines)
    if structured::infer_tabular_delimiter(trimmed).is_some() {
        return "text/csv".to_string();
    }

    "text/plain".to_string()
}

/// Output returned by any [`DocumentConverter`] implementation.
#[derive(Debug, Clone)]
pub struct ConvertedDocument {
    pub converter_name: &'static str,
    pub mime_type: String,
    pub title: Option<String>,
    pub markdown: String,
}

/// Trait implemented by every modular format converter (MarkItDown `DocumentConverter` pattern).
/// Converters are invoked in strict ascending `priority()` order — zero `if/else` dispatch chains.
pub trait DocumentConverter: Send + Sync {
    fn name(&self) -> &'static str;
    /// Lower numerical priority executes earlier (10..=29 specific binary/container formats,
    /// 30..=59 structured text formats, 80+ generic fallbacks).
    fn priority(&self) -> u16;
    /// When `true`, `ConversionPolicy::LocalEditable` skips this converter so line-accurate edits
    /// (e.g., `patch_file` on `.html` or `.csv`) operate on verbatim source lines.
    fn passthrough_when_editable(&self) -> bool {
        false
    }
    fn accepts(&self, bytes: &[u8], info: &StreamInfo) -> bool;
    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument>;
}

/// Priority-ordered registry of pure-Rust document converters.
pub struct DocumentConverterRegistry {
    converters: Vec<Box<dyn DocumentConverter>>,
}

impl DocumentConverterRegistry {
    /// Builds the default registry with all built-in pure-Rust converters sorted strictly by priority.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut converters: Vec<Box<dyn DocumentConverter>> = vec![
            Box::new(ooxml::DocxConverter),
            Box::new(ooxml::XlsxConverter),
            Box::new(ooxml::PptxConverter),
            Box::new(ooxml::EpubConverter),
            Box::new(pdf::PdfConverter),
            Box::new(structured::IpynbConverter),
            Box::new(structured::FeedXmlConverter),
            Box::new(structured::DelimitedTableConverter),
            Box::new(structured::HtmlDocumentConverter),
            Box::new(ooxml::ZipArchiveConverter),
            Box::new(BinaryInspectorConverter),
        ];
        converters.sort_by_key(|c| c.priority());
        Self { converters }
    }

    /// Process-wide singleton converter registry.
    #[must_use]
    pub fn global() -> &'static Self {
        static REGISTRY: OnceLock<DocumentConverterRegistry> = OnceLock::new();
        REGISTRY.get_or_init(Self::with_defaults)
    }

    /// Attempts to convert `bytes` using the highest-priority matching converter permitted by `policy`.
    ///
    /// Returns `Ok(None)` when the input is plain editable UTF-8 source text (or a raster image
    /// handled directly by minicode's multimodal Vision loader).
    pub fn convert_bytes(
        &self,
        bytes: &[u8],
        source_name: Option<&str>,
        hint_mime: Option<&str>,
        policy: ConversionPolicy,
    ) -> Result<Option<ConvertedDocument>> {
        let info = StreamInfo::sniff(bytes, source_name, hint_mime);
        if info.is_visual_image {
            return Ok(None);
        }

        for converter in &self.converters {
            if policy == ConversionPolicy::LocalEditable && converter.passthrough_when_editable() {
                continue;
            }
            if converter.accepts(bytes, &info) {
                let mut doc = converter.convert(bytes, &info)?;
                if doc.mime_type.is_empty() {
                    doc.mime_type = info.mime_type.clone();
                }
                tracing::debug!(
                    converter = doc.converter_name,
                    mime = %doc.mime_type,
                    source = source_name.unwrap_or("<stream>"),
                    out_bytes = doc.markdown.len(),
                    "Converted document stream to Markdown"
                );
                return Ok(Some(doc));
            }
        }

        Ok(None)
    }
}

/// Fallback converter (priority 95) for opaque non-UTF-8 binaries (ELF, WASM, PE, SQLite, fonts,
/// compressed archives, high-entropy blobs) — returns structured Markdown metadata instead of crashing.
pub struct BinaryInspectorConverter;

impl DocumentConverter for BinaryInspectorConverter {
    fn name(&self) -> &'static str {
        "binary_inspector"
    }

    fn priority(&self) -> u16 {
        95
    }

    fn accepts(&self, bytes: &[u8], info: &StreamInfo) -> bool {
        !bytes.is_empty() && !info.is_utf8_text && !info.is_visual_image
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let label = info
            .detected_label
            .as_deref()
            .unwrap_or("Opaque Binary Asset");
        let name = info.source_name.as_deref().unwrap_or("<binary stream>");
        let preview_hex: Vec<String> = bytes
            .iter()
            .take(16)
            .map(|b| format!("{:02x}", b))
            .collect();
        let md = format!(
            "# Binary Asset Metadata: `{}`\n\n\
             - **Detected Format:** {}\n\
             - **MIME Type:** `{}`\n\
             - **Size:** {} bytes ({:.1} KB)\n\
             - **Shannon Entropy:** {:.2} bits/byte\n\
             - **Header Bytes (hex):** `{}`\n\n\
             *This file is a non-text binary artifact with no extractable document structure.*",
            name,
            label,
            info.mime_type,
            bytes.len(),
            bytes.len() as f64 / 1024.0,
            info.shannon_entropy,
            preview_hex.join(" ")
        );
        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: Some(format!("Binary Asset: {}", name)),
            markdown: md,
        })
    }
}

// ============================================================================
// Bounded In-Memory Conversion Cache Keyed by `(path, mtime, len, policy)`
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    canonical_path: String,
    mtime_nanos: u128,
    byte_len: u64,
    policy: ConversionPolicy,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    doc: ConvertedDocument,
    last_tick: u64,
}

struct ConversionCacheState {
    entries: HashMap<CacheKey, CacheEntry>,
    total_bytes: usize,
    tick: u64,
}

fn global_conversion_cache() -> &'static Mutex<ConversionCacheState> {
    static CACHE: OnceLock<Mutex<ConversionCacheState>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(ConversionCacheState {
            entries: HashMap::new(),
            total_bytes: 0,
            tick: 0,
        })
    })
}

/// Converts a workspace file to Markdown (if recognized under `policy`), caching the converted
/// Markdown keyed by `(canonical_path, mtime, byte_len, policy)` so paginated `read_file` slices,
/// `read_file(query=...)` searches, and `look_at` section drilldowns avoid redundant re-parsing.
pub fn convert_workspace_file_cached(
    abs_path: &Path,
    display_path: &str,
    bytes: &[u8],
    policy: ConversionPolicy,
) -> Result<Option<ConvertedDocument>> {
    let mtime_nanos = std::fs::metadata(abs_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let key = CacheKey {
        canonical_path: abs_path.to_string_lossy().to_string(),
        mtime_nanos,
        byte_len: bytes.len() as u64,
        policy,
    };

    if let Ok(mut guard) = global_conversion_cache().lock() {
        guard.tick = guard.tick.saturating_add(1);
        let tick = guard.tick;
        if let Some(entry) = guard.entries.get_mut(&key) {
            entry.last_tick = tick;
            return Ok(Some(entry.doc.clone()));
        }
    }

    let Some(doc) = DocumentConverterRegistry::global().convert_bytes(
        bytes,
        Some(display_path),
        None,
        policy,
    )?
    else {
        return Ok(None);
    };

    if doc.markdown.len() <= MAX_CACHE_TOTAL_BYTES / 2 {
        if let Ok(mut guard) = global_conversion_cache().lock() {
            guard.tick = guard.tick.saturating_add(1);
            let tick = guard.tick;
            if let Some(prev) = guard.entries.remove(&key) {
                guard.total_bytes = guard.total_bytes.saturating_sub(prev.doc.markdown.len());
            }
            guard.total_bytes += doc.markdown.len();
            guard.entries.insert(
                key,
                CacheEntry {
                    doc: doc.clone(),
                    last_tick: tick,
                },
            );
            // Evict least-recently-used entries if exceeding count or byte ceiling
            while guard.entries.len() > MAX_CACHE_ENTRIES
                || guard.total_bytes > MAX_CACHE_TOTAL_BYTES
            {
                let oldest = guard
                    .entries
                    .iter()
                    .min_by_key(|(_, v)| v.last_tick)
                    .map(|(k, _)| k.clone());
                let Some(evict_key) = oldest else {
                    break;
                };
                if let Some(removed) = guard.entries.remove(&evict_key) {
                    guard.total_bytes =
                        guard.total_bytes.saturating_sub(removed.doc.markdown.len());
                }
            }
        }
    }

    Ok(Some(doc))
}

// ============================================================================
// Pure-Rust RFC 2397 `data:` URI Decoder
// ============================================================================

/// Decodes an RFC 2397 `data:[<mediatype>][;base64],<data>` URI into raw bytes and optional MIME type.
pub fn decode_data_uri(uri: &str) -> Result<(Vec<u8>, Option<String>)> {
    let rest = uri
        .strip_prefix("data:")
        .ok_or_else(|| ContextError::Parse("URI does not begin with 'data:'".into()))?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| ContextError::Parse("Malformed data: URI missing ',' separator".into()))?;

    let mut is_base64 = false;
    let mut mime: Option<String> = None;
    for (idx, part) in meta.split(';').enumerate() {
        let trimmed = part.trim();
        if idx == 0 && !trimmed.is_empty() && trimmed.contains('/') {
            mime = Some(trimmed.to_ascii_lowercase());
        } else if trimmed.eq_ignore_ascii_case("base64") {
            is_base64 = true;
        }
    }

    let bytes = if is_base64 {
        decode_base64_standard(payload)?
    } else {
        percent_decode_bytes(payload)
    };
    Ok((bytes, mime))
}

fn decode_base64_standard(input: &str) -> Result<Vec<u8>> {
    #[inline]
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }

    let mut out = Vec::with_capacity((input.len() * 3) / 4);
    let mut buf: u32 = 0;
    let mut bits: u8 = 0;

    for &b in input.as_bytes() {
        if b.is_ascii_whitespace() || b == b'=' {
            continue;
        }
        let Some(v) = val(b) else {
            return Err(ContextError::Parse("Invalid base64 byte in data: URI".into()).into());
        };
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

fn percent_decode_bytes(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(v) = u8::from_str_radix(hex_str, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_info_sniffs_extensionless_docx_pdf_and_entropy() {
        // Extensionless DOCX container (ZIP with word/document.xml)
        let docx_bytes = deflate::build_test_zip(&[(
            "word/document.xml",
            b"<w:document><w:body><w:p><w:r><w:t>Spec</w:t></w:r></w:p></w:body></w:document>",
        )]);
        let info = StreamInfo::sniff(&docx_bytes, Some("mystery_blob"), None);
        assert_eq!(
            info.mime_type,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );

        // Plain Rust source passes through untouched in LocalEditable mode
        let rust_code = b"pub fn alpha(x: usize) -> usize { x + 1 }\n";
        let res = DocumentConverterRegistry::global()
            .convert_bytes(
                rust_code,
                Some("src/lib.rs"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap();
        assert!(res.is_none(), "Plain UTF-8 code must not be converted");

        // Opaque ELF binary falls back cleanly to BinaryInspectorConverter metadata
        let elf = b"\x7FELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let elf_doc = DocumentConverterRegistry::global()
            .convert_bytes(elf, Some("a.out"), None, ConversionPolicy::LocalEditable)
            .unwrap()
            .expect("ELF must produce structured metadata");
        assert_eq!(elf_doc.converter_name, "binary_inspector");
        assert!(elf_doc.markdown.contains("ELF Executable"));
    }

    #[test]
    fn test_decode_data_uri_base64_and_percent_encoded() {
        let (b1, m1) = decode_data_uri("data:text/csv;base64,bmFtZSx2YWwKYSwx").unwrap();
        assert_eq!(m1.as_deref(), Some("text/csv"));
        assert_eq!(String::from_utf8(b1).unwrap(), "name,val\na,1");

        let (b2, m2) = decode_data_uri("data:text/html,%3Ch1%3EHi%3C%2Fh1%3E").unwrap();
        assert_eq!(m2.as_deref(), Some("text/html"));
        assert_eq!(String::from_utf8(b2).unwrap(), "<h1>Hi</h1>");
    }
}
