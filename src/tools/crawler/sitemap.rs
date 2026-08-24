use crate::error::{Result, ToolError};
use crate::tools::crawler::types::SitemapEntry;
use scraper::{Html, Selector};
use url::Url;

/// Parser for sitemaps, sitemap indexes, and llms.txt endpoints.
pub struct SitemapParser;

impl SitemapParser {
    /// Extracts sitemap URLs from raw XML text.
    pub fn parse_xml(xml_content: &str) -> Vec<SitemapEntry> {
        let mut entries = Vec::new();
        let fragment = Html::parse_document(xml_content);

        // Standard sitemap: <url><loc>...</loc><lastmod>...</lastmod></url>
        if let Ok(url_sel) = Selector::parse("url") {
            let loc_sel = Selector::parse("loc").ok();
            let lastmod_sel = Selector::parse("lastmod").ok();

            for el in fragment.select(&url_sel) {
                let loc = loc_sel
                    .as_ref()
                    .and_then(|s| el.select(s).next())
                    .map(|e| e.text().collect::<String>().trim().to_string());
                let lastmod = lastmod_sel
                    .as_ref()
                    .and_then(|s| el.select(s).next())
                    .map(|e| e.text().collect::<String>().trim().to_string());

                if let Some(l) = loc {
                    if !l.is_empty() {
                        entries.push(SitemapEntry { loc: l, lastmod });
                    }
                }
            }
        }

        // Sitemap Index: <sitemap><loc>...</loc></sitemap>
        if entries.is_empty() {
            if let Ok(sitemap_sel) = Selector::parse("sitemap") {
                let loc_sel = Selector::parse("loc").ok();
                for el in fragment.select(&sitemap_sel) {
                    if let Some(loc) = loc_sel
                        .as_ref()
                        .and_then(|s| el.select(s).next())
                        .map(|e| e.text().collect::<String>().trim().to_string())
                    {
                        if !loc.is_empty() {
                            entries.push(SitemapEntry { loc, lastmod: None });
                        }
                    }
                }
            }
        }

        // Fallback regex/simple scan if tags weren't parsed by HTML parser due to XML namespaces
        if entries.is_empty() {
            let re_loc = regex::Regex::new(r"<loc>\s*(https?://[^\s<]+)\s*</loc>").ok();
            if let Some(re) = re_loc {
                for cap in re.captures_iter(xml_content) {
                    if let Some(m) = cap.get(1) {
                        entries.push(SitemapEntry {
                            loc: m.as_str().to_string(),
                            lastmod: None,
                        });
                    }
                }
            }
        }

        entries
    }

    /// Fetches and parses a remote sitemap.xml or auto-discovers it from base URL.
    pub async fn fetch_sitemap(
        client: &reqwest::Client,
        target_url: &str,
    ) -> Result<Vec<SitemapEntry>> {
        let mut candidate_urls = Vec::new();

        if target_url.ends_with(".xml") {
            candidate_urls.push(target_url.to_string());
        } else {
            if let Ok(parsed) = Url::parse(target_url) {
                if let Some(origin) = parsed.origin().ascii_serialization().into() {
                    candidate_urls.push(format!("{}/sitemap.xml", origin));
                    candidate_urls.push(format!("{}/sitemap_index.xml", origin));
                }
            }
            candidate_urls.push(format!("{}/sitemap.xml", target_url.trim_end_matches('/')));
        }

        for u in candidate_urls {
            if let Ok(resp) = client.get(&u).send().await {
                if resp.status().is_success() {
                    if let Ok(text) = resp.text().await {
                        let parsed = Self::parse_xml(&text);
                        if !parsed.is_empty() {
                            return Ok(parsed);
                        }
                    }
                }
            }
        }

        Err(ToolError::CommandExec(format!(
            "Failed to discover or parse sitemap XML from `{}`",
            target_url
        ))
        .into())
    }

    /// Probes for `/llms.txt` or `/llms-full.txt` documentation summaries.
    pub async fn probe_llms_txt(client: &reqwest::Client, base_url: &str) -> Option<String> {
        let parsed = Url::parse(base_url).ok()?;
        let origin = parsed.origin().ascii_serialization();

        let candidates = [
            format!("{}/llms.txt", origin),
            format!("{}/llms-full.txt", origin),
            format!("{}/llms.txt", base_url.trim_end_matches('/')),
        ];

        for u in candidates {
            if let Ok(resp) = client.get(&u).send().await {
                if resp.status().is_success() {
                    if let Ok(text) = resp.text().await {
                        if !text.trim().is_empty() && !text.contains("<html") {
                            return Some(text);
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sitemap_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                <url>
                    <loc>https://example.com/docs/intro</loc>
                    <lastmod>2026-08-01</lastmod>
                </url>
                <url>
                    <loc>https://example.com/docs/api</loc>
                    <lastmod>2026-08-02</lastmod>
                </url>
            </urlset>
        "#;

        let entries = SitemapParser::parse_xml(xml);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].loc, "https://example.com/docs/intro");
        assert_eq!(entries[1].loc, "https://example.com/docs/api");
    }
}
