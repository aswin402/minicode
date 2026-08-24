use crate::tools::browser::markdown::SmartMarkdownExtractor;
use scraper::{Html, Selector};
use url::Url;

/// Distills raw HTML into a clean, boilerplate-free Markdown document.
pub struct MarkdownDistiller;

impl MarkdownDistiller {
    /// Extracts the page title from <title>, <h1>, or <meta property="og:title">.
    pub fn extract_title(html: &str) -> String {
        let fragment = Html::parse_document(html);

        // 1. Try <title>
        if let Ok(sel) = Selector::parse("title") {
            if let Some(el) = fragment.select(&sel).next() {
                let t = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
                if !t.is_empty() {
                    return t;
                }
            }
        }

        // 2. Try <h1>
        if let Ok(sel) = Selector::parse("h1") {
            if let Some(el) = fragment.select(&sel).next() {
                let t = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
                if !t.is_empty() {
                    return t;
                }
            }
        }

        "Untitled Page".to_string()
    }

    /// Converts raw HTML into high-density Markdown, stripping boilerplate elements.
    pub fn distill_to_markdown(html: &str, _base_url: &str) -> String {
        SmartMarkdownExtractor::extract_fit_markdown(html, None)
    }

    /// Extracts all internal links from HTML matching the domain/origin boundary.
    pub fn extract_links(
        html: &str,
        current_url: &str,
        base_origin: &str,
        path_prefix: Option<&str>,
    ) -> Vec<String> {
        let fragment = Html::parse_document(html);
        let base_parsed = match Url::parse(current_url) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut links = Vec::new();
        if let Ok(sel) = Selector::parse("a[href]") {
            for el in fragment.select(&sel) {
                if let Some(href) = el.value().attr("href") {
                    let trimmed = href.trim();
                    if trimmed.is_empty()
                        || trimmed.starts_with('#')
                        || trimmed.starts_with("javascript:")
                        || trimmed.starts_with("mailto:")
                        || trimmed.starts_with("tel:")
                    {
                        continue;
                    }

                    // Resolve relative link
                    if let Ok(resolved) = base_parsed.join(trimmed) {
                        let mut resolved_str = resolved.to_string();
                        // Strip fragment (#section)
                        if let Some(idx) = resolved_str.find('#') {
                            resolved_str.truncate(idx);
                        }
                        // Strip trailing slash for normalization (unless root)
                        if resolved_str.len() > 10 && resolved_str.ends_with('/') {
                            resolved_str.pop();
                        }

                        // Check domain boundary
                        if resolved_str.starts_with(base_origin) {
                            // Check path prefix if specified
                            if let Some(prefix) = path_prefix {
                                if !resolved_str.contains(prefix) {
                                    continue;
                                }
                            }

                            // Filter out binary / media extensions
                            if !Self::is_binary_or_asset(&resolved_str) {
                                links.push(resolved_str);
                            }
                        }
                    }
                }
            }
        }

        links.sort();
        links.dedup();
        links
    }

    fn is_binary_or_asset(url: &str) -> bool {
        let lower = url.to_lowercase();
        let blacklisted = [
            ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".ico", ".pdf", ".zip", ".tar",
            ".gz", ".tgz", ".wasm", ".css", ".js", ".mjs", ".woff", ".woff2", ".ttf", ".eot",
            ".mp4", ".mp3", ".avi", ".mov",
        ];
        blacklisted
            .iter()
            .any(|ext| lower.ends_with(ext) || lower.contains(&format!("{}?", ext)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title_and_links() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Tokio Documentation</title></head>
            <body>
                <main>
                    <h1>Asynchronous Programming with Tokio</h1>
                    <p>Tokio is an async runtime for Rust.</p>
                    <a href="/tokio/tutorial/index.html">Tutorial</a>
                    <a href="https://other.com/escape">External Link</a>
                    <a href="/tokio/image.png">Diagram</a>
                </main>
            </body>
            </html>
        "#;

        let title = MarkdownDistiller::extract_title(html);
        assert_eq!(title, "Tokio Documentation");

        let links = MarkdownDistiller::extract_links(
            html,
            "https://docs.rs/tokio/latest/tokio/index.html",
            "https://docs.rs",
            Some("/tokio/"),
        );

        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "https://docs.rs/tokio/tutorial/index.html");
    }
}
