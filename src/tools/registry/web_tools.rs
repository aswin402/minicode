use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::parse_u64_param;
use crate::tools::web;
use serde_json::json;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "fetch_or_browse".to_string(),
            description: "Fetch web documentation or public web pages and convert HTML to readable Markdown.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The full HTTP/HTTPS URL to fetch"
                    }
                },
                "required": ["url"]
            }),
        },
        ToolSchema {
            name: "search_web".to_string(),
            description: "Search the web for up-to-date documentation, API references, library examples, and programming solutions using search engine queries.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search keywords or query string"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of search results to return (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSchema {
            name: "browser_navigate".to_string(),
            description: "Navigate to a web page or local development server (e.g. http://localhost:3000) and extract an interactive ARIA accessibility tree with numbered element references (@e1, @e2).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The web URL or localhost address to navigate to"
                    }
                },
                "required": ["url"]
            }),
        },
        ToolSchema {
            name: "browser_snapshot".to_string(),
            description: "Capture an accessible ARIA DOM snapshot of a given HTML string or URL to inspect interactive UI components.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL of the page"
                    },
                    "html": {
                        "type": "string",
                        "description": "Raw HTML string to parse into accessibility tree (optional)"
                    }
                },
                "required": ["url"]
            }),
        },
    ]
}

pub async fn dispatch(tool_name: &str, args: &serde_json::Value) -> Option<Result<String>> {
    match tool_name {
        "fetch_or_browse" => Some(
            async {
                let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "fetch_or_browse".to_string(),
                        reason: "Missing required argument 'url'".to_string(),
                    }
                })?;
                web::fetch_or_browse(url).await
            }
            .await,
        ),
        "search_web" => Some(
            async {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "search_web".to_string(),
                        reason: "Missing required argument 'query'".to_string(),
                    }
                })?;
                let max_results = parse_u64_param(args.get("max_results")).unwrap_or(5) as usize;
                let results_md =
                    crate::tools::web_search::WebSearchService::search(query, max_results).await?;
                Ok(results_md)
            }
            .await,
        ),
        "browser_navigate" => Some(
            async {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_navigate".to_string(),
                        reason: "Missing 'url'".to_string(),
                    })?;
                let snapshot = crate::tools::browser::BrowserController::navigate(url).await?;
                let report =
                    crate::tools::browser::BrowserController::format_snapshot_report(&snapshot);
                Ok(report)
            }
            .await,
        ),
        "browser_snapshot" => Some(
            async {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_snapshot".to_string(),
                        reason: "Missing 'url'".to_string(),
                    })?;
                let html_opt = args["html"].as_str();
                let snapshot = if let Some(html) = html_opt {
                    crate::tools::browser::BrowserController::parse_html_to_aria_snapshot(url, html)
                } else {
                    crate::tools::browser::BrowserController::navigate(url).await?
                };
                let report =
                    crate::tools::browser::BrowserController::format_snapshot_report(&snapshot);
                Ok(report)
            }
            .await,
        ),
        _ => None,
    }
}
