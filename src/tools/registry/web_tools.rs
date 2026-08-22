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
            description: "Click an interactive element identified by its ARIA reference (@v1:e1) and return the updated page accessibility tree snapshot immediately in the same turn.".to_string(),
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
            description: "Type text into an input, textarea, or contenteditable element by reference (@v1:e2) and return the updated page accessibility tree snapshot.".to_string(),
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
            description: "Scroll the browser viewport in a given direction ('up', 'down', 'top', 'bottom').".to_string(),
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
            description: "Inspect live browser runtime diagnostics including console logs (errors/warnings), uncaught JS exceptions, and failed HTTP network requests (4xx/5xx).".to_string(),
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
            description: "Evaluate arbitrary JavaScript code in the browser context (e.g. inspecting window state, cookies, local storage, or React/DOM properties) and return the output.".to_string(),
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
            description: "Capture a viewport screenshot of the currently active browser page as a PNG image and save it to the workspace .minicode/screenshots/ directory.".to_string(),
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
        _ => None,
    }
}
