use crate::constants::{WEB_TIMEOUT_SECS, WEB_USER_AGENT};
use crate::error::{Result, ToolError};
use crate::tools::crawler::markdown::MarkdownDistiller;
use crate::tools::crawler::sitemap::SitemapParser;
use crate::tools::crawler::types::{CrawlReport, CrawledPage, CrawlerConfig};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Semaphore};
use url::Url;

/// High-performance bounded BFS documentation crawler.
pub struct CrawlerEngine {
    client: reqwest::Client,
}

impl Default for CrawlerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CrawlerEngine {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(WEB_TIMEOUT_SECS))
            .user_agent(WEB_USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .unwrap_or_default();

        Self { client }
    }

    /// Recursively crawls documentation starting from `root_url` with bounded depth and page limits.
    pub async fn crawl(&self, root_url: &str, config: CrawlerConfig) -> Result<CrawlReport> {
        let start_time = Instant::now();
        let parsed_root = Url::parse(root_url).map_err(|e| ToolError::InvalidArguments {
            name: "crawl_documentation".to_string(),
            reason: format!("Invalid root URL `{}`: {}", root_url, e),
        })?;

        let base_origin = parsed_root.origin().ascii_serialization();
        let path_prefix = parsed_root.path().trim_end_matches('/');
        let path_prefix_opt = if path_prefix.len() > 1 {
            Some(path_prefix)
        } else {
            None
        };

        // 1. Check for llms.txt shortcut
        if let Some(llms_md) = SitemapParser::probe_llms_txt(&self.client, root_url).await {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let page = CrawledPage {
                url: format!("{}/llms.txt", base_origin),
                title: format!(
                    "{} (llms.txt documentation)",
                    parsed_root.host_str().unwrap_or("Documentation")
                ),
                depth: 0,
                markdown: llms_md.clone(),
                char_count: llms_md.chars().count(),
                fetched_at_secs: now,
            };

            return Ok(CrawlReport {
                root_url: root_url.to_string(),
                total_pages_crawled: 1,
                total_chars: page.char_count,
                pages: vec![page],
                skipped_urls_count: 0,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        // 2. Initialize BFS Queue and Visited Tracker
        let visited = Arc::new(Mutex::new(HashSet::new()));
        let pages_collected = Arc::new(Mutex::new(Vec::new()));
        let queue = Arc::new(Mutex::new(VecDeque::new()));

        {
            let mut v_lock = visited.lock().await;
            v_lock.insert(root_url.to_string());
            let mut q_lock = queue.lock().await;
            q_lock.push_back((root_url.to_string(), 0usize));
        }

        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
        let mut total_skipped = 0usize;

        // BFS Loop
        while pages_collected.lock().await.len() < config.max_pages {
            let next_item = {
                let mut q_lock = queue.lock().await;
                q_lock.pop_front()
            };

            let (current_url, depth) = match next_item {
                Some(item) => item,
                None => break, // Queue exhausted
            };

            if depth > config.max_depth {
                total_skipped += 1;
                continue;
            }

            let permit = match semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };

            let client = self.client.clone();
            let visited_clone = visited.clone();
            let queue_clone = queue.clone();
            let pages_clone = pages_collected.clone();
            let base_origin_clone = base_origin.clone();
            let path_prefix_clone = path_prefix_opt.map(|s| s.to_string());
            let query_filter = config.query_filter.clone();

            // Fetch and process page
            let fetch_res = client
                .get(&current_url)
                .header(
                    "Accept",
                    "text/html,application/xhtml+xml,text/plain;q=0.9,text/markdown;q=0.8",
                )
                .send()
                .await;

            drop(permit);

            match fetch_res {
                Ok(resp) if resp.status().is_success() => {
                    let text = resp.text().await.unwrap_or_default();
                    if text.is_empty() {
                        continue;
                    }

                    let title = MarkdownDistiller::extract_title(&text);
                    let md = MarkdownDistiller::distill_to_markdown(&text, &current_url);

                    // Check query relevance if filter is provided
                    let is_relevant = match &query_filter {
                        Some(q) => {
                            let q_lower = q.to_lowercase();
                            title.to_lowercase().contains(&q_lower)
                                || md.to_lowercase().contains(&q_lower)
                        }
                        None => true,
                    };

                    if is_relevant && !md.is_empty() {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let char_count = md.chars().count();
                        let page = CrawledPage {
                            url: current_url.clone(),
                            title,
                            depth,
                            markdown: md,
                            char_count,
                            fetched_at_secs: now,
                        };

                        let mut p_lock = pages_clone.lock().await;
                        p_lock.push(page);
                    }

                    // Discover children links if depth limit not reached
                    if depth < config.max_depth {
                        let links = MarkdownDistiller::extract_links(
                            &text,
                            &current_url,
                            &base_origin_clone,
                            path_prefix_clone.as_deref(),
                        );

                        let mut v_lock = visited_clone.lock().await;
                        let mut q_lock = queue_clone.lock().await;

                        for link in links {
                            if !v_lock.contains(&link) {
                                v_lock.insert(link.clone());
                                q_lock.push_back((link, depth + 1));
                            }
                        }
                    }
                }
                _ => {
                    total_skipped += 1;
                }
            }
        }

        let collected = pages_collected.lock().await.clone();
        let total_chars = collected.iter().map(|p| p.char_count).sum();

        Ok(CrawlReport {
            root_url: root_url.to_string(),
            total_pages_crawled: collected.len(),
            total_chars,
            pages: collected,
            skipped_urls_count: total_skipped,
            duration_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    /// Saves a crawled documentation report to `.minicode/crawled/` in the workspace.
    pub fn save_to_disk(&self, report: &CrawlReport, workspace_root: &Path) -> Result<PathBuf> {
        let dir = workspace_root.join(".minicode").join("crawled");
        let _ = fs::create_dir_all(&dir);

        let sanitized_host = Url::parse(&report.root_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.replace(':', "_")))
            .unwrap_or_else(|| "docs".to_string());

        let filename = format!("{}_crawl.json", sanitized_host);
        let path = dir.join(filename);

        let json_data = serde_json::to_string_pretty(report)
            .map_err(|e| ToolError::CommandExec(e.to_string()))?;

        fs::write(&path, json_data).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;

        Ok(path)
    }

    /// Searches all locally cached crawled reports for a specific query.
    pub fn search_cached_docs(
        workspace_root: &Path,
        query: &str,
        limit: usize,
    ) -> Vec<(CrawledPage, f32)> {
        let dir = workspace_root.join(".minicode").join("crawled");
        if !dir.exists() {
            return Vec::new();
        }

        let mut results = Vec::new();
        let q_lower = query.to_lowercase();
        let terms: Vec<&str> = q_lower.split_whitespace().collect();

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(report) = serde_json::from_str::<CrawlReport>(&content) {
                            for page in report.pages {
                                let mut score = 0.0f32;
                                let title_lower = page.title.to_lowercase();
                                let md_lower = page.markdown.to_lowercase();

                                for term in &terms {
                                    if title_lower.contains(term) {
                                        score += 3.0;
                                    }
                                    if md_lower.contains(term) {
                                        score += 1.0;
                                    }
                                }

                                if score > 0.0 {
                                    results.push((page, score));
                                }
                            }
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }
}
