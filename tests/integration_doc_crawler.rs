/// Integration tests for Phase 44: Deep Recursive Web Crawler & Documentation Ingestion Engine
///
/// Tests XML sitemap parsing, Fit-Markdown boilerplate distillation, bounded link extraction,
/// disk persistence, and cached offline documentation search.
use minicode::tools::crawler::markdown::MarkdownDistiller;
use minicode::tools::crawler::sitemap::SitemapParser;
use minicode::tools::crawler::types::{CrawlReport, CrawledPage};
use minicode::tools::crawler::CrawlerEngine;
use minicode::tools::registry::web_tools;
use tempfile::tempdir;

#[test]
fn test_sitemap_xml_parsing_robustness() {
    let sitemap_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
        <url>
            <loc>https://docs.rs/tokio/latest/tokio/index.html</loc>
            <lastmod>2026-08-20T12:00:00Z</lastmod>
        </url>
        <url>
            <loc>https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html</loc>
            <lastmod>2026-08-21T08:30:00Z</lastmod>
        </url>
        <url>
            <loc>https://docs.rs/tokio/latest/tokio/task/fn.spawn.html</loc>
        </url>
    </urlset>"#;

    let entries = SitemapParser::parse_xml(sitemap_xml);
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries[0].loc,
        "https://docs.rs/tokio/latest/tokio/index.html"
    );
    assert_eq!(entries[0].lastmod.as_deref(), Some("2026-08-20T12:00:00Z"));
    assert_eq!(
        entries[1].loc,
        "https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html"
    );
    assert_eq!(
        entries[2].loc,
        "https://docs.rs/tokio/latest/tokio/task/fn.spawn.html"
    );
}

#[test]
fn test_fit_markdown_distillation_and_link_boundary() {
    let raw_html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Tokio Async Runtime Tutorial</title>
        </head>
        <body>
            <header><nav><a href="/home">Home</a></nav></header>
            <main class="markdown-body">
                <h1>Getting Started with Tokio</h1>
                <p>Tokio is an event-driven, non-blocking I/O platform for writing asynchronous applications.</p>
                <pre><code>async fn main() { println!("hello"); }</code></pre>
                <a href="https://docs.rs/tokio/latest/tokio/sync/index.html">Sync Primitives</a>
                <a href="/tokio/latest/tokio/task/index.html">Task Spawning</a>
                <a href="https://google.com/search">External Search Engine</a>
                <a href="/assets/diagram.png">Architecture Diagram</a>
            </main>
            <footer><p>&copy; 2026 Tokio Contributors</p></footer>
        </body>
        </html>
    "#;

    let title = MarkdownDistiller::extract_title(raw_html);
    assert_eq!(title, "Tokio Async Runtime Tutorial");

    let md =
        MarkdownDistiller::distill_to_markdown(raw_html, "https://docs.rs/tokio/latest/tokio/");
    assert!(md.contains("Getting Started with Tokio"));
    assert!(md.contains("async fn main()"));

    // Verify bounded internal link extraction
    let links = MarkdownDistiller::extract_links(
        raw_html,
        "https://docs.rs/tokio/latest/tokio/index.html",
        "https://docs.rs",
        Some("/tokio/latest/tokio/"),
    );

    // Should include sync and task, but exclude external google.com and .png image
    assert_eq!(links.len(), 2);
    assert!(links.contains(&"https://docs.rs/tokio/latest/tokio/sync/index.html".to_string()));
    assert!(links.contains(&"https://docs.rs/tokio/latest/tokio/task/index.html".to_string()));
    assert!(!links.iter().any(|l| l.contains("google.com")));
    assert!(!links.iter().any(|l| l.ends_with(".png")));
}

#[test]
fn test_crawled_doc_cache_persistence_and_search() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let page1 = CrawledPage {
        url: "https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html".to_string(),
        title: "Semaphore in tokio::sync - Rust".to_string(),
        depth: 1,
        markdown: "# Semaphore\n\nAn asynchronous counting semaphore for limiting concurrency."
            .to_string(),
        char_count: 75,
        fetched_at_secs: 1000,
    };

    let page2 = CrawledPage {
        url: "https://docs.rs/tokio/latest/tokio/sync/struct.RwLock.html".to_string(),
        title: "RwLock in tokio::sync - Rust".to_string(),
        depth: 1,
        markdown: "# RwLock\n\nAn asynchronous reader-writer lock supporting concurrent readers."
            .to_string(),
        char_count: 80,
        fetched_at_secs: 1000,
    };

    let report = CrawlReport {
        root_url: "https://docs.rs/tokio/latest/tokio/".to_string(),
        total_pages_crawled: 2,
        total_chars: 155,
        pages: vec![page1, page2],
        skipped_urls_count: 0,
        duration_ms: 120,
    };

    let engine = CrawlerEngine::new();
    let saved_path = engine.save_to_disk(&report, ws).unwrap();
    assert!(saved_path.exists());

    // Search cached docs
    let matches = CrawlerEngine::search_cached_docs(ws, "counting semaphore", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].0.title, "Semaphore in tokio::sync - Rust");

    let lock_matches = CrawlerEngine::search_cached_docs(ws, "reader-writer lock", 5);
    assert_eq!(lock_matches.len(), 1);
    assert_eq!(lock_matches[0].0.title, "RwLock in tokio::sync - Rust");
}

#[test]
fn test_crawler_schemas_registered_in_registry() {
    let schemas = web_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"crawl_documentation".to_string()));
    assert!(names.contains(&"crawl_sitemap".to_string()));
    assert!(names.contains(&"search_crawled_docs".to_string()));
}
