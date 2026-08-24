use serde::{Deserialize, Serialize};

/// A single extracted and distilled documentation page from a crawl.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrawledPage {
    pub url: String,
    pub title: String,
    pub depth: usize,
    pub markdown: String,
    pub char_count: usize,
    pub fetched_at_secs: u64,
}

/// Comprehensive report containing all crawled pages and statistics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrawlReport {
    pub root_url: String,
    pub total_pages_crawled: usize,
    pub total_chars: usize,
    pub pages: Vec<CrawledPage>,
    pub skipped_urls_count: usize,
    pub duration_ms: u64,
}

/// Configuration parameters for bounded recursive crawling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    pub max_depth: usize,
    pub max_pages: usize,
    pub max_concurrency: usize,
    pub timeout_secs: u64,
    pub query_filter: Option<String>,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_pages: 10,
            max_concurrency: 4,
            timeout_secs: 15,
            query_filter: None,
        }
    }
}

/// A parsed entry from a sitemap XML or documentation index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SitemapEntry {
    pub loc: String,
    pub lastmod: Option<String>,
}
