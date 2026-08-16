use minicode::tools::web_search::{SearchResult, WebSearchService};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn test_search_results_markdown_formatting() {
    let results = vec![
        SearchResult {
            title: "Tokio Runtime Guide".to_string(),
            url: "https://tokio.rs".to_string(),
            snippet: "An asynchronous runtime for the Rust programming language.".to_string(),
        },
        SearchResult {
            title: "Ratatui TUI Framework".to_string(),
            url: "https://ratatui.rs".to_string(),
            snippet: "A Rust library for building rich terminal user interfaces.".to_string(),
        },
    ];

    let formatted = WebSearchService::format_results(&results, "rust async tui");
    assert!(formatted.contains("🔍 Web Search Results for \"rust async tui\":"));
    assert!(formatted.contains("1. **[Tokio Runtime Guide](https://tokio.rs)**"));
    assert!(formatted.contains("2. **[Ratatui TUI Framework](https://ratatui.rs)**"));
}

#[tokio::test]
async fn test_search_web_tool_dispatch_empty_query_rejected() {
    let ws = PathBuf::from("/workspace");
    let result = ToolRegistry::dispatch(
        &ws,
        "call_search_empty",
        "search_web",
        &json!({ "query": "   " }),
        None,
        1,
    )
    .await;

    assert!(!result.success);
    assert!(result.output.contains("cannot be empty"));
}
