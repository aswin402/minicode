use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::browser::{BrowserController, BrowserMode};
use crate::tools::param::*;
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
            name: "browser_batch".to_string(),
            description: "Execute an atomic pipeline of multiple browser actions sequentially in a single turn without round-tripping to LLM between each step. Supports navigate, click, fill, scroll, wait_for_selector, wait_for_network_idle, delay, eval_js, and assert_text.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "actions": {
                        "type": "array",
                        "description": "Ordered array of browser action objects to execute sequentially (e.g. [{\"action\": \"navigate\", \"url\": \"...\"}, {\"action\": \"fill\", \"ref\": \"@v1:e2\", \"text\": \"alice\"}, {\"action\": \"click\", \"ref\": \"@v1:e3\"}, {\"action\": \"wait_for_selector\", \"selector\": \".dashboard\"}])",
                        "items": {
                            "type": "object"
                        }
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
                    }
                },
                "required": ["actions"]
            }),
        },
        ToolSchema {
            name: "browser_mock_route".to_string(),
            description: "Intercept and mock HTTP/API requests over CDP (or block ad/tracking URLs) to test frontend error boundaries (e.g. 500/404/401 handling) and loading states without modifying server code.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "URL pattern or wildcard to intercept (e.g. '*/api/users*', 'https://api.example.com/*')"
                    },
                    "status": {
                        "type": "integer",
                        "description": "Mocked HTTP status code to return (e.g. 200, 401, 404, 500). Default: 200"
                    },
                    "body": {
                        "type": "string",
                        "description": "Mocked HTTP response body string (usually JSON)"
                    },
                    "content_type": {
                        "type": "string",
                        "description": "Response Content-Type header (default: 'application/json')"
                    },
                    "clear": {
                        "type": "boolean",
                        "description": "If true, clears all active mock routes instead of adding a new rule"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "browser_debug_bundle".to_string(),
            description: "Aggregate live browser diagnostics into a unified report: uncaught JavaScript exceptions, console logs, failed HTTP network requests (4xx/5xx / blocked requests), and current active page ARIA tree.".to_string(),
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
            name: "browser_qa_audit".to_string(),
            description: "Run an automated in-page Quality Assurance audit on a target web application or local dev server. Checks for broken images, broken/placeholder links, form accessibility, missing meta/viewport tags, runtime JavaScript exceptions, and 4xx/5xx network failures.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Optional URL to navigate to and audit (defaults to the currently open browser page)"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "browser_inspect_dom".to_string(),
            description: "Perform deep in-page DOM grounding inspection. Pierces Shadow DOM to discover custom Web Components, computes live bounding box geometry (X, Y, W, H), checks element visibility, and extracts precise CSS selectors.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
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
        ToolSchema {
            name: "browser_emulate".to_string(),
            description: "Configure device viewport emulation (mobile, iphone, tablet, desktop, desktop_wide, reset) and network throttling (offline, slow_3g, fast_3g, cable, reset) over CDP.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "viewport": {
                        "type": "string",
                        "enum": ["mobile", "mobile_large", "iphone", "tablet", "ipad", "desktop", "desktop_wide", "reset"],
                        "description": "Device viewport preset"
                    },
                    "network": {
                        "type": "string",
                        "enum": ["offline", "slow_3g", "fast_3g", "cable", "wifi", "reset"],
                        "description": "Network throttling condition preset"
                    },
                    "width": {
                        "type": "integer",
                        "description": "Custom viewport width in pixels"
                    },
                    "height": {
                        "type": "integer",
                        "description": "Custom viewport height in pixels"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "browser_state".to_string(),
            description: "Save or restore browser session states (cookies and localStorage) to/from persistent profiles in .minicode/browser_state/ to maintain logins across agent runs.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["save", "restore"],
                        "description": "Action to perform: 'save' or 'restore'"
                    },
                    "profile": {
                        "type": "string",
                        "description": "Profile name (e.g. 'auth_user', 'admin_session')"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
                    }
                },
                "required": ["action", "profile"]
            }),
        },
        ToolSchema {
            name: "browser_pdf".to_string(),
            description: "Render and export the active web page or target document to a high-fidelity PDF file via CDP Page.printToPDF.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Target file path relative to workspace root (optional, defaults to .minicode/reports/page_export_<timestamp>.pdf)"
                    },
                    "url": {
                        "type": "string",
                        "description": "Optional URL to navigate to before exporting to PDF"
                    },
                    "landscape": {
                        "type": "boolean",
                        "description": "Paper orientation: true for landscape, false for portrait (default: false)"
                    },
                    "print_background": {
                        "type": "boolean",
                        "description": "Print background graphics and colors (default: true)"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "browser_check_injection".to_string(),
            description: "Scan active page DOM for hidden elements (CSS display:none, visibility:hidden, zero-size text, off-screen coords) and audit for prompt injection attacks and context poisoning.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Optional URL to navigate to before scanning"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["headless", "gui"],
                        "description": "Browser execution mode ('headless' or 'gui')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "browser_close".to_string(),
            description: "Close and terminate the active browser session (Chrome, Firefox, or Obscura) and reap all associated browser processes and background workers.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

fn parse_browser_mode(args: &serde_json::Value) -> BrowserMode {
    let mode_str = opt_str(args, "mode").unwrap_or("headless");
    BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless)
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "browser_close" => Some(
            async {
                let stopped = crate::tools::browser::BrowserManager::shutdown_live_engine().await?;
                if stopped {
                    Ok(
                        "✔ Browser closed successfully. All browser processes terminated."
                            .to_string(),
                    )
                } else {
                    Ok("ℹ No active browser session was running.".to_string())
                }
            }
            .await,
        ),
        "fetch_or_browse" => Some(
            async {
                let url = require_str(args, "url", "fetch_or_browse")?;
                let query_opt = opt_str(args, "query");
                web::fetch_or_browse(url, query_opt).await
            }
            .await,
        ),
        "search_web" => Some(
            async {
                let query = require_str(args, "query", "search_web")?;
                let max_results = opt_usize(args, "max_results", 5);
                let results_md =
                    crate::tools::web_search::WebSearchService::search(query, max_results).await?;
                Ok(results_md)
            }
            .await,
        ),
        "browser_navigate" => Some(
            async {
                let url = require_str(args, "url", "browser_navigate")?;
                let mode = parse_browser_mode(args);

                let snapshot =
                    BrowserController::navigate_and_snapshot(url, mode, workspace_root).await?;
                let report = BrowserController::format_snapshot_report(&snapshot);
                Ok(report)
            }
            .await,
        ),
        "browser_snapshot" => Some(
            async {
                let url = require_str(args, "url", "browser_snapshot")?;
                let mode = parse_browser_mode(args);

                let html_opt = opt_str(args, "html");
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
                let target_ref = require_str(args, "ref", "browser_click")?;
                let mode = parse_browser_mode(args);

                BrowserController::click_and_snapshot(target_ref, mode, workspace_root).await
            }
            .await,
        ),
        "browser_fill" => Some(
            async {
                let target_ref = require_str(args, "ref", "browser_fill")?;
                let text = require_str(args, "text", "browser_fill")?;
                let mode = parse_browser_mode(args);

                BrowserController::fill_and_snapshot(target_ref, text, mode, workspace_root).await
            }
            .await,
        ),
        "browser_scroll" => Some(
            async {
                let direction = opt_str(args, "direction").unwrap_or("down");
                let mode = parse_browser_mode(args);

                BrowserController::scroll(direction, mode, workspace_root).await
            }
            .await,
        ),
        "browser_debug_logs" => Some(
            async {
                let mode = parse_browser_mode(args);

                BrowserController::get_debug_logs(mode, workspace_root).await
            }
            .await,
        ),
        "browser_eval" => Some(
            async {
                let script = require_str(args, "script", "browser_eval")?;
                let mode = parse_browser_mode(args);

                BrowserController::evaluate_js(script, mode, workspace_root).await
            }
            .await,
        ),
        "browser_screenshot" => Some(
            async {
                let path_opt = opt_str(args, "path");
                let mode = parse_browser_mode(args);

                BrowserController::take_screenshot(mode, workspace_root, path_opt).await
            }
            .await,
        ),
        "browser_batch" => Some(
            async {
                let mode = parse_browser_mode(args);
                let actions_val = args.get("actions").ok_or_else(|| {
                    crate::error::ToolError::InvalidArguments {
                        name: "browser_batch".to_string(),
                        reason: "Missing required 'actions' array".to_string(),
                    }
                })?;
                let steps: Vec<crate::tools::browser::BatchStep> =
                    serde_json::from_value(actions_val.clone()).map_err(|e| {
                        crate::error::ToolError::InvalidArguments {
                            name: "browser_batch".to_string(),
                            reason: format!("Failed parsing batch actions: {}", e),
                        }
                    })?;

                BrowserController::execute_batch(&steps, mode, workspace_root).await
            }
            .await,
        ),
        "browser_mock_route" => Some(
            async {
                let mode = parse_browser_mode(args);
                let clear = args.get("clear").and_then(|c| c.as_bool()).unwrap_or(false);
                let pattern = opt_str(args, "pattern").unwrap_or("*");
                let status = args.get("status").and_then(|s| s.as_u64()).unwrap_or(200) as u16;
                let body = opt_str(args, "body").unwrap_or("{}");
                let content_type = opt_str(args, "content_type");

                BrowserController::mock_route(
                    pattern,
                    status,
                    body,
                    content_type,
                    clear,
                    mode,
                    workspace_root,
                )
                .await
            }
            .await,
        ),
        "browser_debug_bundle" => Some(
            async {
                let mode = parse_browser_mode(args);
                BrowserController::get_debug_bundle(mode, workspace_root).await
            }
            .await,
        ),
        "browser_qa_audit" => Some(
            async {
                let mode = parse_browser_mode(args);
                let url_opt = opt_str(args, "url");
                BrowserController::run_qa_audit(url_opt, mode, workspace_root).await
            }
            .await,
        ),
        "browser_inspect_dom" => Some(
            async {
                let mode = parse_browser_mode(args);
                BrowserController::inspect_visual_dom(mode, workspace_root).await
            }
            .await,
        ),
        "crawl_documentation" => Some(
            async {
                let url = require_str(args, "url", "crawl_documentation")?;
                let max_depth = opt_usize(args, "max_depth", 2);
                let max_pages = opt_usize(args, "max_pages", 8).min(25);
                let query_filter = opt_str(args, "query").map(|s| s.to_string());

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
                let url = require_str(args, "url", "crawl_sitemap")?;
                let max_links = opt_usize(args, "max_links", 20);

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
                let query = require_str(args, "query", "search_crawled_docs")?;
                let limit = opt_usize(args, "limit", 5);

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
        "browser_emulate" => Some(
            async {
                let mode = parse_browser_mode(args);
                let viewport = opt_str(args, "viewport");
                let network = opt_str(args, "network");
                let width = opt_u64(args, "width").map(|w| w as u32);
                let height = opt_u64(args, "height").map(|h| h as u32);

                BrowserController::emulate_device_and_network(
                    viewport,
                    network,
                    width,
                    height,
                    mode,
                    workspace_root,
                )
                .await
            }
            .await,
        ),
        "browser_state" => Some(
            async {
                let mode = parse_browser_mode(args);
                let action = require_str(args, "action", "browser_state")?;
                let profile = require_str(args, "profile", "browser_state")?;

                match action {
                    "save" => {
                        BrowserController::save_session_state(profile, mode, workspace_root).await
                    }
                    "restore" => {
                        BrowserController::restore_session_state(profile, mode, workspace_root)
                            .await
                    }
                    other => Err(crate::error::ToolError::InvalidArguments {
                        name: "browser_state".to_string(),
                        reason: format!("Unknown action '{}'. Expected 'save' or 'restore'", other),
                    }
                    .into()),
                }
            }
            .await,
        ),
        "browser_pdf" => Some(
            async {
                let mode = parse_browser_mode(args);
                if let Some(target_url) = opt_str(args, "url") {
                    BrowserController::navigate_and_snapshot(target_url, mode, workspace_root)
                        .await?;
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                let custom_path = opt_str(args, "path");
                let landscape = args
                    .get("landscape")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let print_bg = args
                    .get("print_background")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                BrowserController::export_pdf(
                    custom_path,
                    landscape,
                    print_bg,
                    mode,
                    workspace_root,
                )
                .await
            }
            .await,
        ),
        "browser_check_injection" => Some(
            async {
                let mode = parse_browser_mode(args);
                if let Some(target_url) = opt_str(args, "url") {
                    BrowserController::navigate_and_snapshot(target_url, mode, workspace_root)
                        .await?;
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                }

                BrowserController::check_prompt_injections(mode, workspace_root).await
            }
            .await,
        ),
        _ => None,
    }
}
