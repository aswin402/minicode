//! Pure-Rust OOXML (`.docx`, `.xlsx`, `.pptx`), `.epub`, and `.zip` archive converters
//! built on our zero-dependency RFC 1951 Inflate + ZIP Central Directory + XML pull-parser engines.

use super::deflate::{ZipArchive, DEFAULT_MAX_INFLATE_BYTES};
use super::xml::{parse_xml_tree, XmlChild, XmlNode};
use super::{ConvertedDocument, DocumentConverter, StreamInfo};
use crate::error::{ContextError, Result};
use std::collections::HashMap;

const MAX_XML_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const MAX_XLSX_RENDER_ROWS: usize = 80;

// ============================================================================
// 1. DOCX Converter (`word/document.xml`)
// ============================================================================

pub struct DocxConverter;

impl DocumentConverter for DocxConverter {
    fn name(&self) -> &'static str {
        "docx"
    }

    fn priority(&self) -> u16 {
        10
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        info.mime_type == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            || info.zip_entries.iter().any(|e| e == "word/document.xml")
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let archive = ZipArchive::parse(bytes)?;
        let doc_xml = archive.read_entry_string("word/document.xml", MAX_XML_ENTRY_BYTES)?;
        let root = parse_xml_tree(&doc_xml)
            .ok_or_else(|| ContextError::Parse("Malformed word/document.xml".into()))?;

        let body = root.first_child("body").unwrap_or(&root);
        let mut blocks = Vec::new();
        let mut title: Option<String> = None;

        for child in &body.children {
            let XmlChild::Element(el) = child else {
                continue;
            };
            match el.local() {
                "p" => {
                    if let Some((md_line, heading_title)) = render_docx_paragraph(el) {
                        if title.is_none() {
                            if let Some(t) = heading_title {
                                title = Some(t);
                            }
                        }
                        blocks.push(md_line);
                    }
                }
                "tbl" => {
                    if let Some(tbl_md) = render_docx_table(el) {
                        blocks.push(tbl_md);
                    }
                }
                _ => {}
            }
        }

        let mut markdown = blocks.join("\n\n");
        if markdown.trim().is_empty() {
            markdown = "*[Empty DOCX document]*".to_string();
        }

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title,
            markdown,
        })
    }
}

fn render_docx_paragraph(p: &XmlNode) -> Option<(String, Option<String>)> {
    let mut heading_level: Option<usize> = None;
    let mut is_list = false;
    let mut list_level: usize = 0;

    if let Some(ppr) = p.first_child("pPr") {
        if let Some(style) = ppr.first_child("pStyle").and_then(|s| s.attr("val")) {
            let lower = style.to_ascii_lowercase();
            if lower == "title" {
                heading_level = Some(1);
            } else if lower == "subtitle" {
                heading_level = Some(2);
            } else if let Some(rest) = lower.strip_prefix("heading") {
                if let Ok(lvl) = rest.trim().parse::<usize>() {
                    heading_level = Some(lvl.clamp(1, 6));
                }
            } else if lower.contains("list") {
                is_list = true;
            }
        }
        if let Some(numpr) = ppr.first_child("numPr") {
            is_list = true;
            if let Some(ilvl) = numpr
                .first_child("ilvl")
                .and_then(|n| n.attr("val"))
                .and_then(|v| v.parse::<usize>().ok())
            {
                list_level = ilvl.min(6);
            }
        }
    }

    let text = collect_docx_inline_text(p);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(lvl) = heading_level {
        let hashes = "#".repeat(lvl);
        let heading_title = (lvl <= 2).then(|| trimmed.to_string());
        return Some((format!("{} {}", hashes, trimmed), heading_title));
    }

    if is_list {
        let indent = "  ".repeat(list_level);
        return Some((format!("{}- {}", indent, trimmed), None));
    }

    Some((trimmed.to_string(), None))
}

fn collect_docx_inline_text(node: &XmlNode) -> String {
    let mut out = String::new();
    for child in &node.children {
        match child {
            XmlChild::Element(el) => match el.local() {
                "r" => {
                    let mut run_text = String::new();
                    let mut is_bold = false;
                    let mut is_italic = false;
                    let mut is_code = false;
                    if let Some(rpr) = el.first_child("rPr") {
                        is_bold = rpr.first_child("b").is_some_and(|b| {
                            b.attr("val") != Some("0") && b.attr("val") != Some("false")
                        });
                        is_italic = rpr.first_child("i").is_some_and(|i| {
                            i.attr("val") != Some("0") && i.attr("val") != Some("false")
                        });
                        if let Some(rstyle) = rpr.first_child("rStyle").and_then(|s| s.attr("val"))
                        {
                            is_code = rstyle.to_ascii_lowercase().contains("code");
                        }
                    }
                    for rc in &el.children {
                        if let XmlChild::Element(rel) = rc {
                            match rel.local() {
                                "t" => run_text.push_str(&rel.collect_text()),
                                "tab" => run_text.push('\t'),
                                "br" | "cr" => run_text.push('\n'),
                                _ => {}
                            }
                        }
                    }
                    if !run_text.is_empty() {
                        if is_code && !run_text.contains('`') {
                            out.push('`');
                            out.push_str(&run_text);
                            out.push('`');
                        } else if is_bold && is_italic {
                            out.push_str("***");
                            out.push_str(run_text.trim());
                            out.push_str("***");
                        } else if is_bold && !run_text.trim().is_empty() {
                            out.push_str("**");
                            out.push_str(run_text.trim());
                            out.push_str("**");
                            if run_text.ends_with(' ') {
                                out.push(' ');
                            }
                        } else if is_italic && !run_text.trim().is_empty() {
                            out.push('*');
                            out.push_str(run_text.trim());
                            out.push('*');
                            if run_text.ends_with(' ') {
                                out.push(' ');
                            }
                        } else {
                            out.push_str(&run_text);
                        }
                    }
                }
                "hyperlink" | "smartTag" | "sdt" | "sdtContent" | "ins" => {
                    out.push_str(&collect_docx_inline_text(el));
                }
                _ => {}
            },
            XmlChild::Text(_) => {}
        }
    }
    out
}

fn render_docx_table(tbl: &XmlNode) -> Option<String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for tr in tbl.child_elements("tr") {
        let mut cells = Vec::new();
        for tc in tr.child_elements("tc") {
            let mut parts = Vec::new();
            for p in tc.child_elements("p") {
                let txt = collect_docx_inline_text(p);
                let clean = txt.split_whitespace().collect::<Vec<_>>().join(" ");
                if !clean.is_empty() {
                    parts.push(clean);
                }
            }
            cells.push(escape_md_cell(&parts.join(" / ")));
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    format_markdown_table(&rows)
}

// ============================================================================
// 2. XLSX Converter (`xl/workbook.xml` + `sharedStrings.xml` + `sheet*.xml`)
// ============================================================================

pub struct XlsxConverter;

impl DocumentConverter for XlsxConverter {
    fn name(&self) -> &'static str {
        "xlsx"
    }

    fn priority(&self) -> u16 {
        10
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        info.mime_type == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            || info.zip_entries.iter().any(|e| e == "xl/workbook.xml")
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let archive = ZipArchive::parse(bytes)?;

        // 1. Parse optional shared string table (`xl/sharedStrings.xml`)
        let shared_strings = archive
            .read_entry_string("xl/sharedStrings.xml", MAX_XML_ENTRY_BYTES)
            .ok()
            .and_then(|s| parse_xml_tree(&s))
            .map(|root| {
                root.child_elements("si")
                    .map(|si| si.collect_text())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // 2. Parse workbook relationships (`xl/_rels/workbook.xml.rels`) -> rId -> target path
        let mut rid_targets: HashMap<String, String> = HashMap::new();
        if let Ok(rels_xml) =
            archive.read_entry_string("xl/_rels/workbook.xml.rels", MAX_XML_ENTRY_BYTES)
        {
            if let Some(rels_root) = parse_xml_tree(&rels_xml) {
                for rel in rels_root.child_elements("Relationship") {
                    if let (Some(id), Some(target)) = (rel.attr("Id"), rel.attr("Target")) {
                        let clean = target.trim_start_matches('/');
                        let path = if clean.starts_with("xl/") {
                            clean.to_string()
                        } else {
                            format!("xl/{}", clean)
                        };
                        rid_targets.insert(id.to_string(), path);
                    }
                }
            }
        }

        // 3. Resolve ordered sheets from `xl/workbook.xml`
        let mut sheet_specs: Vec<(String, String)> = Vec::new();
        if let Ok(wb_xml) = archive.read_entry_string("xl/workbook.xml", MAX_XML_ENTRY_BYTES) {
            if let Some(wb_root) = parse_xml_tree(&wb_xml) {
                let mut sheet_nodes = Vec::new();
                wb_root.find_all("sheet", &mut sheet_nodes);
                for (idx, sh) in sheet_nodes.iter().enumerate() {
                    let name = sh
                        .attr("name")
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("Sheet{}", idx + 1));
                    let path = sh
                        .attr("id")
                        .and_then(|rid| rid_targets.get(rid).cloned())
                        .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", idx + 1));
                    sheet_specs.push((name, path));
                }
            }
        }

        if sheet_specs.is_empty() {
            let mut ws_paths: Vec<String> = archive
                .entries()
                .iter()
                .filter(|e| e.name.starts_with("xl/worksheets/") && e.name.ends_with(".xml"))
                .map(|e| e.name.clone())
                .collect();
            ws_paths.sort();
            for (idx, path) in ws_paths.into_iter().enumerate() {
                sheet_specs.push((format!("Sheet{}", idx + 1), path));
            }
        }

        let mut sections = Vec::new();
        for (sheet_name, sheet_path) in &sheet_specs {
            let Ok(xml) = archive.read_entry_string(sheet_path, MAX_XML_ENTRY_BYTES) else {
                continue;
            };
            let Some(root) = parse_xml_tree(&xml) else {
                continue;
            };
            let grid = parse_xlsx_sheet_grid(&root, &shared_strings);
            let mut section = format!("## Sheet: {}\n\n", sheet_name);
            if grid.is_empty() {
                section.push_str("*(empty sheet)*");
            } else {
                section.push_str(&render_bounded_table(&grid, MAX_XLSX_RENDER_ROWS));
            }
            sections.push(section);
        }

        let title = info.source_name.clone();
        let markdown = if sections.is_empty() {
            "*[Empty XLSX workbook]*".to_string()
        } else {
            sections.join("\n\n")
        };

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title,
            markdown,
        })
    }
}

/// Converts Excel column letters (`"A"` -> 0, `"Z"` -> 25, `"AA"` -> 26) via base-26 arithmetic.
#[inline]
fn excel_col_index(cell_ref: &str) -> Option<usize> {
    let mut col: usize = 0;
    let mut seen_letter = false;
    for b in cell_ref.bytes() {
        if b.is_ascii_alphabetic() {
            seen_letter = true;
            let digit = (b.to_ascii_uppercase() - b'A') as usize + 1;
            col = col.checked_mul(26)?.checked_add(digit)?;
        } else {
            break;
        }
    }
    seen_letter.then(|| col - 1)
}

fn parse_xlsx_sheet_grid(root: &XmlNode, shared_strings: &[String]) -> Vec<Vec<String>> {
    let Some(sheet_data) = root.first_child("sheetData") else {
        return Vec::new();
    };
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut max_cols = 0usize;

    for row_el in sheet_data.child_elements("row") {
        let mut cells: Vec<(usize, String)> = Vec::new();
        let mut fallback_col = 0usize;

        for cell in row_el.child_elements("c") {
            let col = cell
                .attr("r")
                .and_then(excel_col_index)
                .unwrap_or(fallback_col);
            fallback_col = col + 1;

            let cell_type = cell.attr("t").unwrap_or("");
            let value = match cell_type {
                "s" => cell
                    .first_child("v")
                    .and_then(|v| v.collect_text().trim().parse::<usize>().ok())
                    .and_then(|idx| shared_strings.get(idx))
                    .cloned()
                    .unwrap_or_default(),
                "inlineStr" => cell
                    .first_child("is")
                    .map(|is| is.collect_text())
                    .unwrap_or_default(),
                "b" => match cell.first_child("v").map(|v| v.collect_text()) {
                    Some(v) if v.trim() == "1" => "TRUE".to_string(),
                    _ => "FALSE".to_string(),
                },
                _ => cell
                    .first_child("v")
                    .map(|v| v.collect_text())
                    .unwrap_or_default(),
            };

            let clean = value.split_whitespace().collect::<Vec<_>>().join(" ");
            if !clean.is_empty() {
                max_cols = max_cols.max(col + 1);
                cells.push((col, escape_md_cell(&clean)));
            }
        }

        if !cells.is_empty() {
            let width = cells
                .iter()
                .map(|(c, _)| *c + 1)
                .max()
                .unwrap_or(0)
                .min(128);
            let mut dense = vec![String::new(); width];
            for (c, val) in cells {
                if c < dense.len() {
                    dense[c] = val;
                }
            }
            rows.push(dense);
        }
    }

    let capped_cols = max_cols.min(128);
    for row in &mut rows {
        row.resize(capped_cols, String::new());
    }
    rows
}

// ============================================================================
// 3. PPTX Converter (`ppt/slides/slide*.xml` + speaker notes)
// ============================================================================

pub struct PptxConverter;

impl DocumentConverter for PptxConverter {
    fn name(&self) -> &'static str {
        "pptx"
    }

    fn priority(&self) -> u16 {
        10
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        info.mime_type
            == "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            || info.zip_entries.iter().any(|e| e == "ppt/presentation.xml")
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let archive = ZipArchive::parse(bytes)?;

        // Collect slide XML entries ordered numerically (`slide1.xml`, `slide2.xml`, ..., `slide10.xml`)
        let mut slides: Vec<(usize, String)> = archive
            .entries()
            .iter()
            .filter_map(|e| {
                let stem = e
                    .name
                    .strip_prefix("ppt/slides/slide")?
                    .strip_suffix(".xml")?;
                let num = stem.parse::<usize>().ok()?;
                Some((num, e.name.clone()))
            })
            .collect();
        slides.sort_by_key(|(num, _)| *num);

        let mut out_sections = Vec::new();
        let mut first_title: Option<String> = None;

        for (slide_num, slide_path) in &slides {
            let Ok(xml) = archive.read_entry_string(slide_path, MAX_XML_ENTRY_BYTES) else {
                continue;
            };
            let Some(root) = parse_xml_tree(&xml) else {
                continue;
            };

            let mut paragraphs: Vec<String> = Vec::new();
            let mut p_nodes = Vec::new();
            root.find_all("p", &mut p_nodes);
            for p in p_nodes {
                let mut text_runs = Vec::new();
                let mut t_nodes = Vec::new();
                p.find_all("t", &mut t_nodes);
                for t in t_nodes {
                    text_runs.push(t.collect_text());
                }
                let joined = text_runs.join("");
                let clean = joined.split_whitespace().collect::<Vec<_>>().join(" ");
                if !clean.is_empty() {
                    let lvl = p
                        .first_child("pPr")
                        .and_then(|pr| pr.attr("lvl"))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0)
                        .min(6);
                    if paragraphs.is_empty() {
                        paragraphs.push(clean);
                    } else {
                        paragraphs.push(format!("{}- {}", "  ".repeat(lvl), clean));
                    }
                }
            }

            // Also extract any tables inside the slide (`a:tbl`)
            let mut tbl_nodes = Vec::new();
            root.find_all("tbl", &mut tbl_nodes);
            let mut table_blocks = Vec::new();
            for tbl in tbl_nodes {
                let mut rows = Vec::new();
                for tr in tbl.child_elements("tr") {
                    let mut row = Vec::new();
                    for tc in tr.child_elements("tc") {
                        let mut t_nodes = Vec::new();
                        tc.find_all("t", &mut t_nodes);
                        let txt = t_nodes
                            .iter()
                            .map(|t| t.collect_text())
                            .collect::<Vec<_>>()
                            .join(" ");
                        row.push(escape_md_cell(
                            &txt.split_whitespace().collect::<Vec<_>>().join(" "),
                        ));
                    }
                    if !row.is_empty() {
                        rows.push(row);
                    }
                }
                if let Some(md_tbl) = format_markdown_table(&rows) {
                    table_blocks.push(md_tbl);
                }
            }

            let slide_heading = if let Some(first) = paragraphs.first() {
                if first_title.is_none() {
                    first_title = Some(first.clone());
                }
                format!("## Slide {}: {}", slide_num, first)
            } else {
                format!("## Slide {}", slide_num)
            };

            let mut sec = String::new();
            sec.push_str(&slide_heading);
            if paragraphs.len() > 1 {
                sec.push_str("\n\n");
                sec.push_str(&paragraphs[1..].join("\n"));
            }
            for tb in table_blocks {
                sec.push_str("\n\n");
                sec.push_str(&tb);
            }

            // Speaker notes (`ppt/notesSlides/notesSlideN.xml`)
            let notes_path = format!("ppt/notesSlides/notesSlide{}.xml", slide_num);
            if let Ok(notes_xml) = archive.read_entry_string(&notes_path, MAX_XML_ENTRY_BYTES) {
                if let Some(notes_root) = parse_xml_tree(&notes_xml) {
                    let mut t_nodes = Vec::new();
                    notes_root.find_all("t", &mut t_nodes);
                    let notes_txt = t_nodes
                        .iter()
                        .map(|t| t.collect_text())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let clean_notes = notes_txt.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !clean_notes.is_empty() && clean_notes != slide_num.to_string() {
                        sec.push_str(&format!("\n\n> **Speaker Notes:** {}", clean_notes));
                    }
                }
            }

            out_sections.push(sec);
        }

        let markdown = if out_sections.is_empty() {
            "*[Empty PPTX presentation]*".to_string()
        } else {
            out_sections.join("\n\n")
        };

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: first_title.or_else(|| info.source_name.clone()),
            markdown,
        })
    }
}

// ============================================================================
// 4. EPUB Converter (`META-INF/container.xml` -> OPF `<manifest>` + `<spine>`)
// ============================================================================

pub struct EpubConverter;

impl DocumentConverter for EpubConverter {
    fn name(&self) -> &'static str {
        "epub"
    }

    fn priority(&self) -> u16 {
        12
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        info.mime_type == "application/epub+zip"
            || info
                .zip_entries
                .iter()
                .any(|e| e == "META-INF/container.xml")
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let archive = ZipArchive::parse(bytes)?;
        let container_xml =
            archive.read_entry_string("META-INF/container.xml", MAX_XML_ENTRY_BYTES)?;
        let container_root = parse_xml_tree(&container_xml)
            .ok_or_else(|| ContextError::Parse("Malformed EPUB META-INF/container.xml".into()))?;

        let mut rootfiles = Vec::new();
        container_root.find_all("rootfile", &mut rootfiles);
        let opf_path = rootfiles
            .first()
            .and_then(|rf| rf.attr("full-path"))
            .ok_or_else(|| ContextError::Parse("EPUB OPF rootfile path missing".into()))?
            .to_string();

        let opf_dir = opf_path
            .rsplit_once('/')
            .map(|(dir, _)| format!("{}/", dir))
            .unwrap_or_default();

        let opf_xml = archive.read_entry_string(&opf_path, MAX_XML_ENTRY_BYTES)?;
        let opf_root = parse_xml_tree(&opf_xml)
            .ok_or_else(|| ContextError::Parse("Malformed EPUB OPF manifest".into()))?;

        // Extract title & author metadata
        let mut title_nodes = Vec::new();
        opf_root.find_all("title", &mut title_nodes);
        let book_title = title_nodes
            .first()
            .map(|n| n.collect_text().trim().to_string())
            .filter(|s| !s.is_empty());

        let mut creator_nodes = Vec::new();
        opf_root.find_all("creator", &mut creator_nodes);
        let author = creator_nodes
            .first()
            .map(|n| n.collect_text().trim().to_string())
            .filter(|s| !s.is_empty());

        // Map manifest id -> href
        let mut manifest: HashMap<String, String> = HashMap::new();
        let mut item_nodes = Vec::new();
        opf_root.find_all("item", &mut item_nodes);
        for item in item_nodes {
            if let (Some(id), Some(href)) = (item.attr("id"), item.attr("href")) {
                manifest.insert(id.to_string(), format!("{}{}", opf_dir, href));
            }
        }

        // Walk `<spine>` in exact reading order
        let mut itemrefs = Vec::new();
        opf_root.find_all("itemref", &mut itemrefs);
        let mut chapters = Vec::new();

        if let Some(ref t) = book_title {
            let mut hdr = format!("# {}", t);
            if let Some(ref a) = author {
                hdr.push_str(&format!("\n\n*Author: {}*", a));
            }
            chapters.push(hdr);
        }

        let mut total_chars = 0usize;
        const MAX_EPUB_CHARS: usize = 400_000;
        for itemref in itemrefs {
            let Some(idref) = itemref.attr("idref") else {
                continue;
            };
            let Some(chapter_path) = manifest.get(idref) else {
                continue;
            };
            let Ok(xhtml) = archive.read_entry_string(chapter_path, MAX_XML_ENTRY_BYTES) else {
                continue;
            };
            let md = crate::tools::browser::markdown::SmartMarkdownExtractor::extract_fit_markdown(
                &xhtml, None,
            );
            let trimmed = md.trim();
            if !trimmed.is_empty() {
                total_chars += trimmed.len();
                chapters.push(trimmed.to_string());
                if total_chars >= MAX_EPUB_CHARS {
                    chapters
                        .push("*[EPUB output truncated at 400 KB reading ceiling]*".to_string());
                    break;
                }
            }
        }

        let markdown = if chapters.is_empty() {
            "*[Empty EPUB document]*".to_string()
        } else {
            chapters.join("\n\n---\n\n")
        };

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: book_title,
            markdown,
        })
    }
}

// ============================================================================
// 5. Generic ZIP Archive Converter (Priority 80)
// ============================================================================

pub struct ZipArchiveConverter;

impl DocumentConverter for ZipArchiveConverter {
    fn name(&self) -> &'static str {
        "zip_archive"
    }

    fn priority(&self) -> u16 {
        80
    }

    fn accepts(&self, _bytes: &[u8], info: &StreamInfo) -> bool {
        info.mime_type == "application/zip"
    }

    fn convert(&self, bytes: &[u8], info: &StreamInfo) -> Result<ConvertedDocument> {
        let archive = ZipArchive::parse(bytes)?;
        let label = info.source_name.as_deref().unwrap_or("archive.zip");
        let mut entries: Vec<_> = archive.entries().iter().collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let total_uncompressed: usize = entries.iter().map(|e| e.uncompressed_size).sum();
        let mut out = format!(
            "# ZIP Archive: `{}`\n\n- **Entries:** {}\n- **Total Uncompressed Size:** {} bytes ({:.1} KB)\n\n## Contents\n\n| Entry | Uncompressed | Compressed |\n| --- | --- | --- |\n",
            label,
            entries.len(),
            total_uncompressed,
            total_uncompressed as f64 / 1024.0
        );

        for entry in entries.iter().take(200) {
            out.push_str(&format!(
                "| `{}` | {} B | {} B |\n",
                escape_md_cell(&entry.name),
                entry.uncompressed_size,
                entry.compressed_size
            ));
        }
        if entries.len() > 200 {
            out.push_str(&format!(
                "\n*... {} additional entries omitted ...*\n",
                entries.len() - 200
            ));
        }

        // Preview up to 6 small UTF-8 inner text files (<= 8 KB each) automatically
        let mut previews_emitted = 0usize;
        let mut preview_budget = 24_000usize;
        for entry in &entries {
            if previews_emitted >= 6 || preview_budget == 0 {
                break;
            }
            if entry.name.ends_with('/')
                || entry.uncompressed_size == 0
                || entry.uncompressed_size > 8192
            {
                continue;
            }
            if let Ok(raw) = archive.read_entry(&entry.name, DEFAULT_MAX_INFLATE_BYTES.min(16_384))
            {
                if super::is_clean_utf8_text(&raw) {
                    if let Ok(text) = std::str::from_utf8(&raw) {
                        let snippet = text.trim();
                        if !snippet.is_empty() && snippet.len() <= preview_budget {
                            out.push_str(&format!(
                                "\n## Preview: `{}`\n\n```\n{}\n```\n",
                                entry.name, snippet
                            ));
                            preview_budget = preview_budget.saturating_sub(snippet.len());
                            previews_emitted += 1;
                        }
                    }
                }
            }
        }

        Ok(ConvertedDocument {
            converter_name: self.name(),
            mime_type: info.mime_type.clone(),
            title: Some(format!("ZIP Archive: {}", label)),
            markdown: out,
        })
    }
}

// ============================================================================
// Shared Markdown Table Formatting Helpers
// ============================================================================

#[inline]
pub(crate) fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub(crate) fn format_markdown_table(rows: &[Vec<String>]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return None;
    }

    let mut out = String::new();
    let emit_row = |out: &mut String, row: &[String]| {
        out.push('|');
        for c in 0..cols {
            out.push(' ');
            if let Some(cell) = row.get(c) {
                out.push_str(cell);
            }
            out.push_str(" |");
        }
        out.push('\n');
    };

    emit_row(&mut out, &rows[0]);
    out.push('|');
    for _ in 0..cols {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows.iter().skip(1) {
        emit_row(&mut out, row);
    }
    Some(out.trim_end().to_string())
}

pub(crate) fn render_bounded_table(rows: &[Vec<String>], max_rows: usize) -> String {
    if rows.len() <= max_rows {
        return format_markdown_table(rows).unwrap_or_default();
    }
    let head_n = (max_rows * 3) / 4;
    let tail_n = max_rows.saturating_sub(head_n).max(4);
    let mut sampled = Vec::with_capacity(head_n + tail_n + 1);
    sampled.extend_from_slice(&rows[..head_n.min(rows.len())]);
    let omitted = rows.len().saturating_sub(head_n + tail_n);
    if omitted > 0 {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1);
        let mut marker = vec![String::new(); cols];
        marker[0] = format!("... ({} rows omitted) ...", omitted);
        sampled.push(marker);
    }
    sampled.extend_from_slice(&rows[rows.len().saturating_sub(tail_n)..]);
    let mut md = format_markdown_table(&sampled).unwrap_or_default();
    md.push_str(&format!(
        "\n\n*Showing {} of {} rows ({} omitted).* ",
        rows.len() - omitted,
        rows.len(),
        omitted
    ));
    md
}

#[cfg(test)]
mod tests {
    use crate::context::doc_convert::deflate::build_test_zip;
    use crate::context::doc_convert::{ConversionPolicy, DocumentConverterRegistry};

    #[test]
    fn test_docx_xlsx_pptx_epub_and_zip_converters() {
        let reg = DocumentConverterRegistry::global();

        // 1. DOCX
        let docx = build_test_zip(&[(
            "word/document.xml",
            br#"<w:document><w:body>
              <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Architecture</w:t></w:r></w:p>
              <w:p><w:r><w:t>Native </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>Rust</w:t></w:r></w:p>
              <w:tbl>
                <w:tr><w:tc><w:p><w:r><w:t>Key</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Val</w:t></w:r></w:p></w:tc></w:tr>
                <w:tr><w:tc><w:p><w:r><w:t>Mode</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Fast</w:t></w:r></w:p></w:tc></w:tr>
              </w:tbl>
            </w:body></w:document>"#,
        )]);
        let docx_md = reg
            .convert_bytes(
                &docx,
                Some("spec.docx"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .unwrap();
        assert_eq!(docx_md.converter_name, "docx");
        assert!(docx_md.markdown.contains("# Architecture"));
        assert!(docx_md.markdown.contains("**Rust**"));
        assert!(docx_md.markdown.contains("| Key | Val |"));

        // 2. XLSX (with sharedStrings + base-26 cell coords)
        let xlsx = build_test_zip(&[
            (
                "xl/workbook.xml",
                br#"<workbook><sheets><sheet name="Metrics" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                br#"<sst><si><t>Metric</t></si><si><t>LatencyMs</t></si><si><t>Search</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                br#"<worksheet><sheetData>
                  <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
                  <row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>4.2</v></c></row>
                </sheetData></worksheet>"#,
            ),
        ]);
        let xlsx_md = reg
            .convert_bytes(
                &xlsx,
                Some("bench.xlsx"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .unwrap();
        assert_eq!(xlsx_md.converter_name, "xlsx");
        assert!(xlsx_md.markdown.contains("## Sheet: Metrics"));
        assert!(xlsx_md.markdown.contains("| Search | 4.2 |"));

        // 3. PPTX (with slide + speaker notes)
        let pptx = build_test_zip(&[
            ("ppt/presentation.xml", b"<p:presentation/>"),
            (
                "ppt/slides/slide1.xml",
                br#"<p:sld><p:cSld><p:spTree>
                  <p:sp><p:txBody><a:p><a:r><a:t>Roadmap</a:t></a:r></a:p><a:p><a:r><a:t>Ship pure Rust</a:t></a:r></a:p></p:txBody></p:sp>
                </p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                br#"<p:notes><a:t>Highlight zero dependencies</a:t></p:notes>"#,
            ),
        ]);
        let pptx_md = reg
            .convert_bytes(
                &pptx,
                Some("deck.pptx"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .unwrap();
        assert_eq!(pptx_md.converter_name, "pptx");
        assert!(pptx_md.markdown.contains("## Slide 1: Roadmap"));
        assert!(pptx_md.markdown.contains("Ship pure Rust"));
        assert!(pptx_md.markdown.contains("Speaker Notes"));

        // 4. EPUB
        let epub = build_test_zip(&[
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#,
            ),
            (
                "OEBPS/content.opf",
                br#"<package>
                  <metadata><dc:title>Systems Guide</dc:title><dc:creator>Ada</dc:creator></metadata>
                  <manifest><item id="ch1" href="ch1.xhtml"/></manifest>
                  <spine><itemref idref="ch1"/></spine>
                </package>"#,
            ),
            (
                "OEBPS/ch1.xhtml",
                b"<html><body><h1>Chapter One</h1><p>Zero-copy parsing.</p></body></html>",
            ),
        ]);
        let epub_md = reg
            .convert_bytes(
                &epub,
                Some("book.epub"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .unwrap();
        assert_eq!(epub_md.converter_name, "epub");
        assert!(epub_md.markdown.contains("# Systems Guide"));
        assert!(epub_md.markdown.contains("Zero-copy parsing."));

        // 5. Generic ZIP archive
        let generic_zip = build_test_zip(&[("README.txt", b"Hello from archive")]);
        let zip_md = reg
            .convert_bytes(
                &generic_zip,
                Some("bundle.zip"),
                None,
                ConversionPolicy::LocalEditable,
            )
            .unwrap()
            .unwrap();
        assert_eq!(zip_md.converter_name, "zip_archive");
        assert!(zip_md.markdown.contains("README.txt"));
        assert!(zip_md.markdown.contains("Hello from archive"));
    }
}
