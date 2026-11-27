//! Pure-Rust zero-dependency streaming XML pull-parser FSM and lightweight DOM builder
//! (used by DOCX, XLSX, PPTX, EPUB OPF, and RSS/Atom feed converters — zero regexes).

use std::collections::HashMap;

/// Maximum element nesting depth tolerated before flattening/truncating hostile XML.
const MAX_XML_DEPTH: usize = 256;

/// Single XML token emitted by the deterministic state-machine parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlEvent {
    StartTag {
        name: String,
        attrs: HashMap<String, String>,
        self_closing: bool,
    },
    EndTag {
        name: String,
    },
    Text(String),
}

/// Lightweight parsed XML element node for structured traversal (OOXML, OPF, RSS/Atom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlNode {
    pub name: String,
    pub attrs: HashMap<String, String>,
    pub children: Vec<XmlChild>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlChild {
    Element(XmlNode),
    Text(String),
}

/// Strips any namespace prefix (`"w:tbl"` -> `"tbl"`, `"dc:title"` -> `"title"`).
#[inline]
#[must_use]
pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

impl XmlNode {
    #[inline]
    #[must_use]
    pub fn local(&self) -> &str {
        local_name(&self.name)
    }

    /// Looks up an attribute by exact key OR namespace-stripped local attribute name.
    #[must_use]
    pub fn attr(&self, key: &str) -> Option<&str> {
        if let Some(v) = self.attrs.get(key) {
            return Some(v.as_str());
        }
        let target_local = local_name(key);
        self.attrs
            .iter()
            .find(|(k, _)| local_name(k) == target_local)
            .map(|(_, v)| v.as_str())
    }

    /// Iterates direct child elements matching a namespace-agnostic local tag name.
    pub fn child_elements<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children.iter().filter_map(move |c| match c {
            XmlChild::Element(el) if el.local() == local => Some(el),
            _ => None,
        })
    }

    /// Returns the first direct child element matching `local`.
    #[must_use]
    pub fn first_child(&self, local: &str) -> Option<&XmlNode> {
        self.children.iter().find_map(|c| match c {
            XmlChild::Element(el) if el.local() == local => Some(el),
            _ => None,
        })
    }

    /// Recursively collects all descendant text content concatenated in document order.
    #[must_use]
    pub fn collect_text(&self) -> String {
        let mut buf = String::new();
        self.append_text(&mut buf);
        buf
    }

    fn append_text(&self, out: &mut String) {
        for child in &self.children {
            match child {
                XmlChild::Text(t) => out.push_str(t),
                XmlChild::Element(el) => el.append_text(out),
            }
        }
    }

    /// Recursively collects all descendant elements matching `local` in DFS document order.
    pub fn find_all<'a>(&'a self, local: &str, out: &mut Vec<&'a XmlNode>) {
        for child in &self.children {
            if let XmlChild::Element(el) = child {
                if el.local() == local {
                    out.push(el);
                }
                el.find_all(local, out);
            }
        }
    }
}

/// Decodes XML/XHTML named and numeric character references (`&amp;`, `&#39;`, `&#x1F600;`, etc.).
#[must_use]
pub fn decode_xml_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    const NAMED_ENTITIES: &[(&str, &str)] = &[
        ("amp", "&"),
        ("lt", "<"),
        ("gt", ">"),
        ("quot", "\""),
        ("apos", "'"),
        ("nbsp", " "),
        ("ndash", "–"),
        ("mdash", "—"),
        ("hellip", "…"),
        ("copy", "©"),
        ("reg", "®"),
        ("trade", "™"),
        ("laquo", "«"),
        ("raquo", "»"),
    ];

    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(rel_semi) = raw[i + 1..].find(';') {
                if rel_semi <= 16 {
                    let entity = &raw[i + 1..i + 1 + rel_semi];
                    let decoded = if let Some(num) = entity.strip_prefix('#') {
                        let code = if let Some(hex) =
                            num.strip_prefix('x').or_else(|| num.strip_prefix('X'))
                        {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            num.parse::<u32>().ok()
                        };
                        code.and_then(char::from_u32).map(|c| c.to_string())
                    } else {
                        NAMED_ENTITIES
                            .iter()
                            .find_map(|&(k, v)| (k == entity).then(|| v.to_string()))
                    };

                    if let Some(replacement) = decoded {
                        out.push_str(&replacement);
                        i += rel_semi + 2;
                        continue;
                    }
                }
            }
        }
        if let Some(ch) = raw[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

/// Tokenizes an XML input string into a flat stream of [`XmlEvent`]s in a single $O(N)$ pass.
#[must_use]
pub fn tokenize_xml(input: &str) -> Vec<XmlEvent> {
    let mut events = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;

    // Skip optional UTF-8 BOM
    if input.starts_with('\u{FEFF}') {
        pos = 3;
    }

    while pos < len {
        if bytes[pos] != b'<' {
            let start = pos;
            while pos < len && bytes[pos] != b'<' {
                pos += 1;
            }
            let text = decode_xml_entities(&input[start..pos]);
            if !text.is_empty() {
                events.push(XmlEvent::Text(text));
            }
            continue;
        }

        // Comment `<!-- ... -->`
        if input[pos..].starts_with("<!--") {
            pos = input[pos + 4..]
                .find("-->")
                .map_or(len, |rel| pos + 4 + rel + 3);
            continue;
        }

        // CDATA `<![CDATA[ ... ]]>`
        if input[pos..].starts_with("<![CDATA[") {
            let start = pos + 9;
            if let Some(rel) = input[start..].find("]]>") {
                let cdata = &input[start..start + rel];
                if !cdata.is_empty() {
                    events.push(XmlEvent::Text(cdata.to_string()));
                }
                pos = start + rel + 3;
            } else {
                pos = len;
            }
            continue;
        }

        // Processing instruction `<? ... ?>`
        if input[pos..].starts_with("<?") {
            pos = input[pos + 2..]
                .find("?>")
                .map_or(len, |rel| pos + 2 + rel + 2);
            continue;
        }

        // DOCTYPE / declaration `<! ... >` (handling optional internal `[ ... ]` subset)
        if input[pos..].starts_with("<!") {
            pos += 2;
            let mut bracket_depth = 0usize;
            while pos < len {
                match bytes[pos] {
                    b'[' => bracket_depth += 1,
                    b']' => bracket_depth = bracket_depth.saturating_sub(1),
                    b'>' if bracket_depth == 0 => {
                        pos += 1;
                        break;
                    }
                    _ => {}
                }
                pos += 1;
            }
            continue;
        }

        // End tag `</name>`
        if input[pos..].starts_with("</") {
            pos += 2;
            let start = pos;
            while pos < len && bytes[pos] != b'>' {
                pos += 1;
            }
            let name = input[start..pos].trim().to_string();
            if pos < len {
                pos += 1;
            }
            if !name.is_empty() {
                events.push(XmlEvent::EndTag { name });
            }
            continue;
        }

        // Start or self-closing tag `<name attr="val" ... [/]>`
        pos += 1; // skip '<'
        let tag_start = pos;
        let mut quote: Option<u8> = None;
        while pos < len {
            let b = bytes[pos];
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
            } else if b == b'"' || b == b'\'' {
                quote = Some(b);
            } else if b == b'>' {
                break;
            }
            pos += 1;
        }
        let raw_tag = input[tag_start..pos].trim();
        if pos < len {
            pos += 1; // skip '>'
        }
        if raw_tag.is_empty() {
            continue;
        }

        let (inner, self_closing) = match raw_tag.strip_suffix('/') {
            Some(stripped) => (stripped.trim_end(), true),
            None => (raw_tag, false),
        };

        let (name, attrs) = parse_tag_header(inner);
        if !name.is_empty() {
            events.push(XmlEvent::StartTag {
                name,
                attrs,
                self_closing,
            });
        }
    }

    events
}

fn parse_tag_header(header: &str) -> (String, HashMap<String, String>) {
    let bytes = header.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;

    while pos < len && !bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let name = header[..pos].to_string();
    let mut attrs = HashMap::new();

    while pos < len {
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= len {
            break;
        }
        let key_start = pos;
        while pos < len && bytes[pos] != b'=' && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let key = header[key_start..pos].trim();
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= len || bytes[pos] != b'=' {
            if !key.is_empty() {
                attrs.insert(key.to_string(), String::new());
            }
            continue;
        }
        pos += 1; // skip '='
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= len {
            break;
        }

        let val = if bytes[pos] == b'"' || bytes[pos] == b'\'' {
            let q = bytes[pos];
            pos += 1;
            let val_start = pos;
            while pos < len && bytes[pos] != q {
                pos += 1;
            }
            let slice = &header[val_start..pos];
            if pos < len {
                pos += 1;
            }
            decode_xml_entities(slice)
        } else {
            let val_start = pos;
            while pos < len && !bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            decode_xml_entities(&header[val_start..pos])
        };

        if !key.is_empty() {
            attrs.insert(key.to_string(), val);
        }
    }

    (name, attrs)
}

/// Builds a lightweight [`XmlNode`] root element tree from XML source text.
#[must_use]
pub fn parse_xml_tree(input: &str) -> Option<XmlNode> {
    let events = tokenize_xml(input);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;

    for ev in events {
        match ev {
            XmlEvent::StartTag {
                name,
                attrs,
                self_closing,
            } => {
                let node = XmlNode {
                    name,
                    attrs,
                    children: Vec::new(),
                };
                if self_closing {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlChild::Element(node));
                    } else if root.is_none() {
                        root = Some(node);
                    }
                } else if stack.len() < MAX_XML_DEPTH {
                    stack.push(node);
                }
            }
            XmlEvent::Text(text) => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(XmlChild::Text(text));
                }
            }
            XmlEvent::EndTag { name } => {
                let target_local = local_name(&name);
                // Unwind to the matching open tag (tolerant of minor XML/HTML mismatches)
                if let Some(match_pos) = stack
                    .iter()
                    .rposition(|n| n.name == name || n.local() == target_local)
                {
                    while stack.len() > match_pos {
                        if let Some(finished) = stack.pop() {
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(XmlChild::Element(finished));
                            } else {
                                root = Some(finished);
                            }
                        }
                    }
                }
            }
        }
    }

    while let Some(finished) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(XmlChild::Element(finished));
        } else {
            root = Some(finished);
        }
    }

    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_tree_namespaces_cdata_and_entity_decoding() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <!-- comment -->
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
              <w:r><w:t>Hello &amp; Welcome &#x2014; <![CDATA[<Raw>]]></w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;

        let root = parse_xml_tree(xml).unwrap();
        assert_eq!(root.local(), "document");
        let body = root.first_child("body").unwrap();
        let p = body.first_child("p").unwrap();
        let style = p.first_child("pPr").unwrap().first_child("pStyle").unwrap();
        assert_eq!(style.attr("val"), Some("Heading1"));
        assert_eq!(p.collect_text().trim(), "Hello & Welcome — <Raw>");
    }
}
