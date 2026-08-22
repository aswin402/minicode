/// Integration tests for Phase 32: Fit Markdown & Intelligent Documentation Ingestion
///
/// Tests HTML-to-Markdown distillation, noise pruning, query filtering,
/// llms.txt probing, SSRF guards, and updated web tools schema.
use minicode::tools::browser::markdown::SmartMarkdownExtractor;
use minicode::tools::registry::web_tools;
use minicode::tools::web;

#[test]
fn test_fit_markdown_html_conversion_and_noise_pruning() {
    let raw_html = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <title>Tokio Async Runtime</title>
            <script>window.__analytics = { id: 12345 };</script>
            <style>body { font-family: sans-serif; }</style>
        </head>
        <body>
            <header>
                <nav>
                    <a href="/home">Home</a>
                    <a href="/docs">Docs</a>
                </nav>
            </header>
            <main>
                <h1>Tokio Overview</h1>
                <p>Tokio is an event-driven, non-blocking I/O platform for writing asynchronous applications with the Rust programming language.</p>
                <h2>Key Components</h2>
                <ul>
                    <li>A multi-threaded, work-stealing based task scheduler.</li>
                    <li>A driver backed by the operating system's event queue (epoll, kqueue, IOCP).</li>
                </ul>
                <pre><code>let mut listener = TcpListener::bind("127.0.0.1:8080").await?;</code></pre>
            </main>
            <footer>
                <p>&copy; 2026 Tokio Contributors. All rights reserved.</p>
            </footer>
        </body>
        </html>
    "#;

    let md = SmartMarkdownExtractor::extract_fit_markdown(raw_html, None);

    assert!(md.contains("Tokio Overview"));
    assert!(md.contains("Tokio is an event-driven"));
    assert!(md.contains("Key Components"));
    assert!(md.contains("scheduler"));
    assert!(md.contains("TcpListener::bind"));

    // Verify noisy blocks were completely stripped
    assert!(!md.contains("__analytics"));
    assert!(!md.contains("font-family"));
    assert!(!md.contains("&copy; 2026 Tokio Contributors"));
}

#[test]
fn test_fit_markdown_query_filtering() {
    let raw_html = r#"
        <div>
            <h1>Backend Framework Documentation</h1>
            <p>Welcome to the framework documentation guide.</p>
            
            <h2>Routing Engine</h2>
            <p>The routing engine matches URL patterns to controller handlers efficiently using a radix tree.</p>

            <h2>Authentication</h2>
            <p>Authentication is handled via JWT tokens passed in the Authorization header.</p>

            <h2>Database ORM</h2>
            <p>The ORM provides query builder capabilities for PostgreSQL and SQLite databases.</p>
        </div>
    "#;

    let md_filtered =
        SmartMarkdownExtractor::extract_fit_markdown(raw_html, Some("database postgresql"));

    // Should contain Database section
    assert!(md_filtered.contains("Database ORM") || md_filtered.contains("PostgreSQL"));
    // Header is preserved
    assert!(md_filtered.contains("Backend Framework Documentation"));
}

#[test]
fn test_fetch_or_browse_blocks_ssrf() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        assert!(web::fetch_or_browse("http://localhost:8080", None)
            .await
            .is_err());
        assert!(web::fetch_or_browse("http://127.0.0.1:3000", None)
            .await
            .is_err());
        assert!(
            web::fetch_or_browse("http://169.254.169.254/latest/meta-data/", None)
                .await
                .is_err()
        );
        assert!(web::fetch_or_browse("http://10.0.0.1/private", None)
            .await
            .is_err());
        assert!(web::fetch_or_browse("ftp://example.com", None)
            .await
            .is_err());
    });
}

#[test]
fn test_fetch_or_browse_schema_includes_query_parameter() {
    let schemas = web_tools::get_schemas();
    let fetch_schema = schemas
        .into_iter()
        .find(|s| s.name == "fetch_or_browse")
        .expect("fetch_or_browse schema must be registered");

    let props = fetch_schema.parameters.get("properties").unwrap();
    assert!(props.get("url").is_some());
    assert!(props.get("query").is_some());
}
