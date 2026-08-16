use crate::constants::WEB_USER_AGENT;
use crate::error::{Result, ToolError};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(15 * 60); // 15 minutes

/// A single structured web search result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Thread-safe in-memory cache for search results to avoid rate limits during multi-turn planning.
static SEARCH_CACHE: Mutex<Option<HashMap<String, (Instant, String)>>> = Mutex::new(None);

pub struct WebSearchService;

impl WebSearchService {
    /// Executes a web search query with caching and multi-provider fallback.
    pub async fn search(query: &str, max_results: usize) -> Result<String> {
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "search_web".to_string(),
                reason: "Query string cannot be empty".to_string(),
            }
            .into());
        }

        // 1. Check in-memory cache
        if let Ok(mut guard) = SEARCH_CACHE.lock() {
            let cache = guard.get_or_insert_with(HashMap::new);
            if let Some((timestamp, cached_output)) = cache.get(query_trimmed) {
                if timestamp.elapsed() < CACHE_TTL {
                    return Ok(format!("ℹ [Cached Result]\n\n{}", cached_output));
                }
            }
        }

        // 2. Check for Tavily / Brave API keys in environment
        if let Ok(tavily_key) = std::env::var("TAVILY_API_KEY") {
            if !tavily_key.trim().is_empty() {
                if let Ok(results) =
                    Self::search_tavily(query_trimmed, &tavily_key, max_results).await
                {
                    let formatted = Self::format_results(&results, query_trimmed);
                    Self::store_in_cache(query_trimmed, &formatted);
                    return Ok(formatted);
                }
            }
        }

        if let Ok(brave_key) = std::env::var("BRAVE_API_KEY") {
            if !brave_key.trim().is_empty() {
                if let Ok(results) =
                    Self::search_brave(query_trimmed, &brave_key, max_results).await
                {
                    let formatted = Self::format_results(&results, query_trimmed);
                    Self::store_in_cache(query_trimmed, &formatted);
                    return Ok(formatted);
                }
            }
        }

        // 3. Fallback: DuckDuckGo HTML Search (Zero-API-key)
        let results = Self::search_duckduckgo(query_trimmed, max_results).await?;
        let formatted = Self::format_results(&results, query_trimmed);
        Self::store_in_cache(query_trimmed, &formatted);

        Ok(formatted)
    }

    /// Searches DuckDuckGo HTML endpoint and parses result snippets.
    async fn search_duckduckgo(query: &str, max_results: usize) -> Result<Vec<SearchResult>> {
        let client = reqwest::Client::builder()
            .user_agent(WEB_USER_AGENT)
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e: reqwest::Error| {
                ToolError::CommandExec(format!("Failed to build HTTP client: {}", e))
            })?;

        let resp: reqwest::Response = client
            .post("https://html.duckduckgo.com/html/")
            .form(&[("q", query), ("b", "")])
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .map_err(|e: reqwest::Error| {
                ToolError::CommandExec(format!("HTTP search request failed: {}", e))
            })?;

        let html_text: String = resp.text().await.map_err(|e: reqwest::Error| {
            ToolError::CommandExec(format!("Failed to read response body: {}", e))
        })?;

        let document = Html::parse_document(&html_text);
        let result_sel = Selector::parse(".result").unwrap();
        let title_sel = Selector::parse(".result__title .result__a").unwrap();
        let snippet_sel = Selector::parse(".result__snippet").unwrap();
        let url_sel = Selector::parse(".result__url").unwrap();

        let mut results = Vec::new();

        for element in document.select(&result_sel) {
            if results.len() >= max_results {
                break;
            }

            let title = element
                .select(&title_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let snippet = element
                .select(&snippet_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let raw_url = element
                .select(&title_sel)
                .next()
                .and_then(|e| e.value().attr("href").map(|s| s.to_string()))
                .or_else(|| {
                    element
                        .select(&url_sel)
                        .next()
                        .map(|e| e.text().collect::<String>())
                })
                .unwrap_or_default()
                .trim()
                .to_string();

            // DuckDuckGo redirects often look like //duckduckgo.com/l/?uddg=https%3A%2F%2F...
            let actual_url = if let Some(pos) = raw_url.find("uddg=") {
                let encoded = &raw_url[pos + 5..];
                let end_pos = encoded.find('&').unwrap_or(encoded.len());
                let clean_encoded = &encoded[..end_pos];
                urlencoding_decode(clean_encoded).unwrap_or(raw_url)
            } else if raw_url.starts_with("//") {
                format!("https:{}", raw_url)
            } else {
                raw_url
            };

            if !title.is_empty() && !actual_url.is_empty() {
                results.push(SearchResult {
                    title,
                    url: actual_url,
                    snippet,
                });
            }
        }

        Ok(results)
    }

    /// Queries Tavily API if key is present.
    async fn search_tavily(
        query: &str,
        api_key: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>> {
        let client = reqwest::Client::new();
        let resp: reqwest::Response = client
            .post("https://api.tavily.com/search")
            .json(&serde_json::json!({
                "api_key": api_key,
                "query": query,
                "max_results": max_results,
                "search_depth": "basic"
            }))
            .send()
            .await
            .map_err(|e: reqwest::Error| ToolError::CommandExec(e.to_string()))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e: reqwest::Error| ToolError::CommandExec(e.to_string()))?;

        let mut results = Vec::new();
        if let Some(items) = data
            .get("results")
            .and_then(|r: &serde_json::Value| r.as_array())
        {
            for item in items {
                let title = item
                    .get("title")
                    .and_then(|t: &serde_json::Value| t.as_str())
                    .unwrap_or_default()
                    .to_string();
                let url = item
                    .get("url")
                    .and_then(|u: &serde_json::Value| u.as_str())
                    .unwrap_or_default()
                    .to_string();
                let snippet = item
                    .get("content")
                    .and_then(|c: &serde_json::Value| c.as_str())
                    .unwrap_or_default()
                    .to_string();
                if !title.is_empty() && !url.is_empty() {
                    results.push(SearchResult {
                        title,
                        url,
                        snippet,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Queries Brave Search API if key is present.
    async fn search_brave(
        query: &str,
        api_key: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>> {
        let client = reqwest::Client::new();
        let resp: reqwest::Response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .query(&[("q", query), ("count", &max_results.to_string())])
            .header("X-Subscription-Token", api_key)
            .send()
            .await
            .map_err(|e: reqwest::Error| ToolError::CommandExec(e.to_string()))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e: reqwest::Error| ToolError::CommandExec(e.to_string()))?;

        let mut results = Vec::new();
        if let Some(items) = data
            .get("web")
            .and_then(|w: &serde_json::Value| w.get("results"))
            .and_then(|r: &serde_json::Value| r.as_array())
        {
            for item in items {
                let title = item
                    .get("title")
                    .and_then(|t: &serde_json::Value| t.as_str())
                    .unwrap_or_default()
                    .to_string();
                let url = item
                    .get("url")
                    .and_then(|u: &serde_json::Value| u.as_str())
                    .unwrap_or_default()
                    .to_string();
                let snippet = item
                    .get("description")
                    .and_then(|d: &serde_json::Value| d.as_str())
                    .unwrap_or_default()
                    .to_string();
                if !title.is_empty() && !url.is_empty() {
                    results.push(SearchResult {
                        title,
                        url,
                        snippet,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Formats search results into clean Markdown with direct links and snippets.
    pub fn format_results(results: &[SearchResult], query: &str) -> String {
        if results.is_empty() {
            return format!("ℹ No search results found for query: \"{}\"", query);
        }

        let mut out = format!("🔍 Web Search Results for \"{}\":\n\n", query);
        for (idx, res) in results.iter().enumerate() {
            out.push_str(&format!(
                "{}. **[{}]({})**\n   {}\n\n",
                idx + 1,
                res.title,
                res.url,
                res.snippet
            ));
        }
        out
    }

    fn store_in_cache(query: &str, formatted_output: &str) {
        if let Ok(mut guard) = SEARCH_CACHE.lock() {
            let cache = guard.get_or_insert_with(HashMap::new);
            cache.insert(
                query.to_string(),
                (Instant::now(), formatted_output.to_string()),
            );
        }
    }
}

/// Helper function to URL decode percent-encoded strings without extra dependencies.
fn urlencoding_decode(input: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next()?;
            let h2 = chars.next()?;
            let slice = [h1, h2];
            let hex_str = std::str::from_utf8(&slice).ok()?;
            let val = u8::from_str_radix(hex_str, 16).ok()?;
            bytes.push(val);
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_results_markdown() {
        let results = vec![
            SearchResult {
                title: "Rust Language".to_string(),
                url: "https://www.rust-lang.org".to_string(),
                snippet: "A language empowering everyone to build reliable and efficient software."
                    .to_string(),
            },
            SearchResult {
                title: "crates.io".to_string(),
                url: "https://crates.io".to_string(),
                snippet: "The Rust package registry.".to_string(),
            },
        ];

        let md = WebSearchService::format_results(&results, "rust language");
        assert!(md.contains("🔍 Web Search Results for \"rust language\":"));
        assert!(md.contains("1. **[Rust Language](https://www.rust-lang.org)**"));
        assert!(md.contains("2. **[crates.io](https://crates.io)**"));
    }

    #[test]
    fn test_urlencoding_decode() {
        let raw = "https%3A%2F%2Fdocs.rs%2Ftokio%2Flatest%2Ftokio%2F";
        let decoded = urlencoding_decode(raw).unwrap();
        assert_eq!(decoded, "https://docs.rs/tokio/latest/tokio/");
    }
}
