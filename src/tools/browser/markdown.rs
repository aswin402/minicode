use crate::constants::{WEB_MAX_BODY_BYTES, WEB_TIMEOUT_SECS, WEB_USER_AGENT};
use crate::error::{Result, ToolError};
use std::time::Duration;

/// Pipeline for token-efficient documentation extraction and noise pruning
pub struct SmartMarkdownExtractor;

impl SmartMarkdownExtractor {
    /// Attempts 3-step smart Markdown ingestion:
    /// 1. Content Negotiation (`Accept: text/markdown`)
    /// 2. `llms.txt` & `llms-full.txt` hierarchy probing
    /// 3. Fast HTML-to-Markdown distillation with noise pruning
    pub async fn fetch_smart_markdown(url: &str, query: Option<&str>) -> Result<String> {
        // Step 1: Content Negotiation
        if let Some(md) = Self::content_negotiate_markdown(url).await {
            tracing::info!(url = %url, "Ingested via Accept: text/markdown content negotiation");
            return Ok(Self::postprocess_markdown(
                &md,
                url,
                "Content-Negotiation (text/markdown)",
            ));
        }

        // Step 2: llms.txt probing
        if let Some(md) = Self::probe_llms_txt(url).await {
            tracing::info!(url = %url, "Ingested via /llms.txt endpoint");
            return Ok(Self::postprocess_markdown(
                &md,
                url,
                "llms.txt documentation",
            ));
        }

        // Step 3: Fast HTML fetch and Fit Markdown conversion
        tracing::info!(url = %url, "Ingesting via HTML-to-Fit-Markdown distillation");
        let html = Self::fetch_raw_html(url).await?;
        let fit_md = Self::extract_fit_markdown(&html, query);
        Ok(Self::postprocess_markdown(
            &fit_md,
            url,
            "Fit Markdown Distiller",
        ))
    }

    /// Negotiates Markdown content-type directly from the web server
    pub async fn content_negotiate_markdown(url: &str) -> Option<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(WEB_TIMEOUT_SECS))
            .user_agent(WEB_USER_AGENT)
            .build()
            .ok()?;

        let resp = client
            .get(url)
            .header("Accept", "text/markdown, text/plain;q=0.9, text/html;q=0.8")
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        if content_type.contains("text/markdown")
            || (content_type.contains("text/plain") && !url.ends_with(".html"))
        {
            let text = resp.text().await.ok()?;
            if !text.trim().is_empty() {
                return Some(text);
            }
        }

        None
    }

    /// Probes for `/llms.txt` and `/llms-full.txt` files on the target domain
    pub async fn probe_llms_txt(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let origin = format!("{}://{}", parsed.scheme(), parsed.host_str()?);
        let port_part = parsed.port().map(|p| format!(":{}", p)).unwrap_or_default();
        let base_origin = format!("{}{}", origin, port_part);

        let candidate_urls = vec![
            format!("{}/llms.txt", base_origin),
            format!("{}/llms-full.txt", base_origin),
            format!("{}/.well-known/llms.txt", base_origin),
        ];

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent(WEB_USER_AGENT)
            .build()
            .ok()?;

        for candidate in candidate_urls {
            if let Ok(resp) = client.get(&candidate).send().await {
                if resp.status().is_success() {
                    let content_type = resp
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");

                    if content_type.contains("text/plain")
                        || content_type.contains("text/markdown")
                        || content_type.is_empty()
                    {
                        if let Ok(text) = resp.text().await {
                            let trimmed = text.trim();
                            if !trimmed.is_empty()
                                && !trimmed.to_lowercase().starts_with("<!doctype")
                            {
                                return Some(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Converts HTML into clean Markdown using the htmd engine with noise filtering
    pub fn extract_fit_markdown(html: &str, query: Option<&str>) -> String {
        // Strip common noisy sections before conversion
        let sanitized = strip_noisy_html_blocks(html);

        // Convert HTML to Markdown using htmd
        let converter = htmd::HtmlToMarkdown::builder()
            .skip_tags(vec![
                "script", "style", "noscript", "svg", "nav", "footer", "aside",
            ])
            .build();

        let raw_md = converter
            .convert(&sanitized)
            .unwrap_or_else(|_| sanitized.clone());

        // Clean up excessive newlines and whitespace
        let cleaned = clean_markdown_formatting(&raw_md);

        // If query is supplied, rank paragraphs and filter out non-relevant noise
        if let Some(q) = query {
            filter_markdown_by_query(&cleaned, q)
        } else {
            cleaned
        }
    }

    /// Fetches raw HTML body from a URL
    async fn fetch_raw_html(url: &str) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(WEB_TIMEOUT_SECS))
            .user_agent(WEB_USER_AGENT)
            .build()
            .map_err(|e| ToolError::CommandExec(format!("HTTP client error: {}", e)))?;

        let response =
            client.get(url).send().await.map_err(|e| {
                ToolError::CommandExec(format!("Failed fetching URL {}: {}", url, e))
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(
                ToolError::CommandExec(format!("HTTP {} response from {}", status, url)).into(),
            );
        }

        let body = response
            .text()
            .await
            .map_err(|e| ToolError::CommandExec(format!("Failed reading response: {}", e)))?;

        Ok(body)
    }

    /// Enforces size budget and adds header metadata
    fn postprocess_markdown(md: &str, url: &str, source_type: &str) -> String {
        let mut out = format!("*Source: `{}` | Ingestion: `{}`*\n\n", url, source_type);
        out.push_str(md.trim());

        if out.len() > WEB_MAX_BODY_BYTES {
            let valid_len = out.floor_char_boundary(WEB_MAX_BODY_BYTES);
            out.truncate(valid_len);
            out.push_str("\n\n_...[content truncated to token budget limit]..._");
        }

        out
    }
}

/// Finds the byte index of an ASCII pattern case-insensitively within a UTF-8 string
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    for i in 0..=haystack_bytes.len().saturating_sub(needle_bytes.len()) {
        if haystack.is_char_boundary(i)
            && haystack.is_char_boundary(i + needle_bytes.len())
            && haystack_bytes[i..i + needle_bytes.len()]
                .iter()
                .zip(needle_bytes.iter())
                .all(|(h, n)| h.eq_ignore_ascii_case(n))
        {
            return Some(i);
        }
    }
    None
}

/// Strips intrusive non-content sections (<header>, <nav>, <footer>, <aside>, <script>, <style>)
fn strip_noisy_html_blocks(html: &str) -> String {
    let mut s = html.to_string();
    let noisy_tags = [
        "script", "style", "noscript", "svg", "header", "nav", "footer", "aside",
    ];

    for tag in noisy_tags {
        let open = format!("<{}", tag);
        let close = format!("</{}>", tag);

        while let Some(start_idx) = find_ascii_ci(&s, &open) {
            let end_rel = match find_ascii_ci(&s[start_idx..], &close) {
                Some(idx) => idx,
                None => break,
            };
            let end_idx = start_idx + end_rel + close.len();
            if s.is_char_boundary(start_idx) && s.is_char_boundary(end_idx) {
                s.replace_range(start_idx..end_idx, " ");
            } else {
                break;
            }
        }
    }

    s
}

/// Normalizes spacing, trims triple+ newlines to double newlines, strips trailing spaces
fn clean_markdown_formatting(md: &str) -> String {
    let lines: Vec<&str> = md.lines().map(|l| l.trim_end()).collect();
    let mut out = Vec::new();
    let mut empty_count = 0;

    for line in lines {
        if line.trim().is_empty() {
            empty_count += 1;
            if empty_count <= 2 {
                out.push("");
            }
        } else {
            empty_count = 0;
            out.push(line);
        }
    }

    out.join("\n")
}

/// Simple keyword filtering keeping sections/paragraphs matching query terms
fn filter_markdown_by_query(md: &str, query: &str) -> String {
    let terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    if terms.is_empty() {
        return md.to_string();
    }

    let paragraphs: Vec<&str> = md.split("\n\n").collect();
    let mut relevant = Vec::new();

    for p in paragraphs {
        let p_lower = p.to_lowercase();
        let matches = terms.iter().any(|t| p_lower.contains(t));
        // Keep headings, code blocks, or paragraphs matching any search term
        if matches || p.starts_with('#') || p.starts_with("```") {
            relevant.push(p.trim());
        }
    }

    if relevant.is_empty() {
        md.to_string()
    } else {
        relevant.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_fit_markdown_removes_scripts_and_nav() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><script>console.log("tracking");</script></head>
            <body>
                <nav><a href="/">Home</a><a href="/pricing">Pricing</a></nav>
                <main>
                    <h1>Tokio Async Runtime</h1>
                    <p>Tokio is an asynchronous runtime for the Rust programming language.</p>
                </main>
                <footer>&copy; 2026 Tokio Contributors</footer>
            </body>
            </html>
        "#;

        let md = SmartMarkdownExtractor::extract_fit_markdown(html, None);
        assert!(md.contains("Tokio Async Runtime"));
        assert!(md.contains("Tokio is an asynchronous runtime"));
        assert!(!md.contains("tracking"));
    }

    #[test]
    fn test_filter_markdown_by_query() {
        let md = r#"# Architecture

This document describes the caching layer in detail.

## Database

PostgreSQL is used for relational storage.

## Authentication

JWT tokens are used for authentication."#;

        let filtered = filter_markdown_by_query(md, "database postgresql");
        assert!(filtered.contains("PostgreSQL is used"));
        assert!(filtered.contains("# Architecture"));
    }

    #[test]
    fn test_clean_markdown_formatting() {
        let raw = "Hello\n\n\n\n\nWorld   \n\nTest";
        let cleaned = clean_markdown_formatting(raw);
        assert!(!cleaned.contains("\n\n\n\n"));
        assert!(cleaned.contains("Hello\n\n\nWorld\n\nTest"));
    }
}
