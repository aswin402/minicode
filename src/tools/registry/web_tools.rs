use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::browser::{BrowserController, BrowserMode};
use crate::tools::parse_u64_param;
use crate::tools::web;
use serde_json::json;
use std::path::Path;
use std::str::FromStr;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "fetch_or_browse".to_string(),
            description: "Fetch web documentation or public web pages and convert HTML to readable Markdown using smart 3-step pipeline (Accept negotiation, llms.txt probing, and Fit Markdown distillation).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The full HTTP/HTTPS URL to fetch"
                    },
                    "query": {
                        "type": "string",
                        "description": "Optional search keywords to focus and filter relevant documentation sections"
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
            description: "Navigate to a web page or local development server (e.g. http://localhost:3000) using multi-engine browser automation (Obscura -> Firefox -> Chrome) and extract an interactive ARIA accessibility tree with numbered element references (@v1:e1, @v1:e2).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The web URL or localhost address to navigate to"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser mode: 'headless' (default, fast/clean background) or 'gui' (visible window for live inspection)"
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
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode if fetching live URL"
                    }
                },
                "required": ["url"]
            }),
        },
        ToolSchema {
            name: "browser_click".to_string(),
            description: "Click an interactive element identified by its ARIA reference (@v1:e1) and return the updated page accessibility tree snapshot immediately in the same turn. Always use element references from the most recent browser tool response; older refs may be stale.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "The ARIA element reference identifier to click (e.g. '@v1:e1' or '@e1')"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser mode ('headless' or 'gui')"
                    }
                },
                "required": ["ref"]
            }),
        },
        ToolSchema {
            name: "browser_fill".to_string(),
            description: "Type text into an input, textarea, or contenteditable element by reference (@v1:e2) and return the updated page accessibility tree snapshot. Always use element references from the most recent browser tool response; older refs may be stale.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "The ARIA element reference identifier to fill (e.g. '@v1:e2')"
                    },
                    "text": {
                        "type": "string",
                        "description": "The text string to type into the form element"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser mode ('headless' or 'gui')"
                    }
                },
                "required": ["ref", "text"]
            }),
        },
        ToolSchema {
            name: "browser_scroll".to_string(),
            description: "Scroll the browser viewport in a given direction ('up', 'down', 'top', 'bottom'). Always use element references from the most recent browser tool response; older refs may be stale.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down", "top", "bottom"],
                        "description": "Scroll direction (default: 'down')"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "browser_debug_logs".to_string(),
            description: "Inspect live browser runtime diagnostics including console logs (errors/warnings), uncaught JS exceptions, and failed HTTP network requests (4xx/5xx). Always use element references from the most recent browser tool response; older refs may be stale.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "browser_eval".to_string(),
            description: "Evaluate arbitrary JavaScript code in the browser context (e.g. inspecting window state, cookies, local storage, or React/DOM properties) and return the output. Always use element references from the most recent browser tool response; older refs may be stale.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "The JavaScript expression or code snippet to execute"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser mode to evaluate in ('headless' or 'gui')"
                    }
                },
                "required": ["script"]
            }),
        },
        ToolSchema {
            name: "browser_screenshot".to_string(),
            description: "Capture a viewport screenshot of the currently active browser page as a PNG image and save it to the workspace .minicode/screenshots/ directory. Always use element references from the most recent browser tool response; older refs may be stale.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional relative path in workspace to save the screenshot image"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "crawl_documentation".to_string(),
            description: "Recursively crawl a documentation site with domain boundaries, depth limits, and max page caps, extracting clean Fit-Markdown into a structured knowledge report and caching to .minicode/crawled/.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Root URL of the documentation site (e.g. 'https://docs.rs/tokio/latest/tokio/')"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum BFS recursion depth (default: 2)"
                    },
                    "max_pages": {
                        "type": "integer",
                        "description": "Maximum total pages to crawl and distill (default: 8, max: 25)"
                    },
                    "query": {
                        "type": "string",
                        "description": "Optional search keyword to prioritize and filter relevant documentation sections"
                    }
                },
                "required": ["url"]
            }),
        },
        ToolSchema {
            name: "crawl_sitemap".to_string(),
            description: "Discover and parse a website's sitemap.xml or /llms.txt to index all available documentation endpoints.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The base website URL or sitemap.xml endpoint"
                    },
                    "max_links": {
                        "type": "integer",
                        "description": "Maximum number of sitemap links to list (default: 20)"
                    }
                },
                "required": ["url"]
            }),
        },
        ToolSchema {
            name: "search_crawled_docs".to_string(),
            description: "Search across all locally cached crawled documentation reports in the workspace without making external network calls.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keywords or search term to look up across previously crawled pages"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of page matches to return (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        },
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "fetch_or_browse" => Some(
            async {
                let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "fetch_or_browse".to_string(),
                        reason: "Missing required argument 'url'".to_string(),
                    }
                })?;
                let query_opt = args.get("query").and_then(|q| q.as_str());
                web::fetch_or_browse(url, query_opt).await
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
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                let snapshot =
                    BrowserController::navigate_and_snapshot(url, mode, workspace_root).await?;
                let report = BrowserController::format_snapshot_report(&snapshot);
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
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                let html_opt = args["html"].as_str();
                let snapshot = if let Some(html) = html_opt {
                    BrowserController::parse_html_to_aria_snapshot(url, html)
                } else {
                    BrowserController::navigate_and_snapshot(url, mode, workspace_root).await?
                };
                let report = BrowserController::format_snapshot_report(&snapshot);
                Ok(report)
            }
            .await,
        ),
        "browser_click" => Some(
            async {
                let target_ref =
                    args["ref"]
                        .as_str()
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "browser_click".to_string(),
                            reason: "Missing 'ref'".to_string(),
                        })?;
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                BrowserController::click_and_snapshot(target_ref, mode, workspace_root).await
            }
            .await,
        ),
        "browser_fill" => Some(
            async {
                let target_ref =
                    args["ref"]
                        .as_str()
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "browser_fill".to_string(),
                            reason: "Missing 'ref'".to_string(),
                        })?;
                let text = args["text"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_fill".to_string(),
                        reason: "Missing 'text'".to_string(),
                    })?;
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                BrowserController::fill_and_snapshot(target_ref, text, mode, workspace_root).await
            }
            .await,
        ),
        "browser_scroll" => Some(
            async {
                let direction = args
                    .get("direction")
                    .and_then(|d| d.as_str())
                    .unwrap_or("down");
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                BrowserController::scroll(direction, mode, workspace_root).await
            }
            .await,
        ),
        "browser_debug_logs" => Some(
            async {
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                BrowserController::get_debug_logs(mode, workspace_root).await
            }
            .await,
        ),
        "browser_eval" => Some(
            async {
                let script =
                    args["script"]
                        .as_str()
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "browser_eval".to_string(),
                            reason: "Missing 'script'".to_string(),
                        })?;
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                BrowserController::evaluate_js(script, mode, workspace_root).await
            }
            .await,
        ),
        "browser_screenshot" => Some(
            async {
                let path_opt = args.get("path").and_then(|p| p.as_str());
                let mode_str = args
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("headless");
                let mode = BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);

                BrowserController::take_screenshot(mode, workspace_root, path_opt).await
            }
            .await,
        ),
        "crawl_documentation" => Some(
            async {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "crawl_documentation".to_string(),
                        reason: "Missing required argument 'url'".to_string(),
                    })?;

                let max_depth =
                    crate::tools::parse_u64_param(args.get("max_depth")).unwrap_or(2) as usize;
                let max_pages = crate::tools::parse_u64_param(args.get("max_pages"))
                    .unwrap_or(8)
                    .min(25) as usize;
                let query_filter = args
                    .get("query")
                    .and_then(|q| q.as_str())
                    .map(|s| s.to_string());

                let config = crate::tools::crawler::CrawlerConfig {
                    max_depth,
                    max_pages,
                    max_concurrency: 4,
                    timeout_secs: 15,
                    query_filter,
                };

                let engine = crate::tools::crawler::CrawlerEngine::new();
                let report = engine.crawl(url, config).await?;
                let saved_path = engine.save_to_disk(&report, workspace_root)?;

                let mut out = format!(
                    "🌐 Crawled {} page(s) from `{}` ({} total chars, took {}ms)\n",
                    report.total_pages_crawled,
                    report.root_url,
                    report.total_chars,
                    report.duration_ms
                );
                out.push_str(&format!(
                    "💾 Cached locally to `{}`\n\n",
                    saved_path.display()
                ));

                for (i, p) in report.pages.iter().enumerate() {
                    out.push_str(&format!(
                        "{}. [{}]({}) — (depth {}, {} chars)\n",
                        i + 1,
                        p.title,
                        p.url,
                        p.depth,
                        p.char_count
                    ));
                }

                if let Some(first) = report.pages.first() {
                    let preview = if first.markdown.len() > 1200 {
                        let limit = first.markdown.floor_char_boundary(1200);
                        format!(
                            "{}...\n*(truncated, {} total chars)*",
                            &first.markdown[..limit],
                            first.char_count
                        )
                    } else {
                        first.markdown.clone()
                    };
                    out.push_str(&format!(
                        "\n### Preview of Root Page (`{}`):\n\n{}\n",
                        first.title, preview
                    ));
                }

                Ok(out)
            }
            .await,
        ),
        "crawl_sitemap" => Some(
            async {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "crawl_sitemap".to_string(),
                        reason: "Missing required argument 'url'".to_string(),
                    })?;
                let max_links =
                    crate::tools::parse_u64_param(args.get("max_links")).unwrap_or(20) as usize;

                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(
                        crate::constants::WEB_TIMEOUT_SECS,
                    ))
                    .user_agent(crate::constants::WEB_USER_AGENT)
                    .build()
                    .unwrap_or_default();

                let entries =
                    crate::tools::crawler::SitemapParser::fetch_sitemap(&client, url).await?;
                let mut out = format!(
                    "🗺 Sitemap Endpoints for `{}` ({} total links):\n\n",
                    url,
                    entries.len()
                );

                for (i, entry) in entries.iter().take(max_links).enumerate() {
                    let mod_str = entry.lastmod.as_deref().unwrap_or("unknown");
                    out.push_str(&format!(
                        "{}. `{}` (lastmod: {})\n",
                        i + 1,
                        entry.loc,
                        mod_str
                    ));
                }

                if entries.len() > max_links {
                    out.push_str(&format!(
                        "\n*...and {} more endpoints.*",
                        entries.len() - max_links
                    ));
                }

                Ok(out)
            }
            .await,
        ),
        "search_crawled_docs" => Some(
            async {
                let query = args["query"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "search_crawled_docs".to_string(),
                        reason: "Missing required argument 'query'".to_string(),
                    })?;
                let limit = crate::tools::parse_u64_param(args.get("limit")).unwrap_or(5) as usize;

                let results = crate::tools::crawler::CrawlerEngine::search_cached_docs(
                    workspace_root,
                    query,
                    limit,
                );
                if results.is_empty() {
                    Ok(format!(
                        "ℹ No cached documentation matches found for `{}` in `.minicode/crawled/`.",
                        query
                    ))
                } else {
                    let mut out = format!(
                        "🔍 Cached Documentation Search Results for `{}` ({} match(es)):\n\n",
                        query,
                        results.len()
                    );
                    for (i, (page, score)) in results.iter().enumerate() {
                        let snippet = if page.markdown.len() > 300 {
                            let limit = page.markdown.floor_char_boundary(300);
                            format!("{}...", &page.markdown[..limit])
                        } else {
                            page.markdown.clone()
                        };
                        out.push_str(&format!(
                            "{}. **{}** — `{}` (Score: {:.1})\n```markdown\n{}\n```\n\n",
                            i + 1,
                            page.title,
                            page.url,
                            score,
                            snippet.trim()
                        ));
                    }
                    Ok(out)
                }
            }
            .await,
        ),
        _ => None,
    }
}
