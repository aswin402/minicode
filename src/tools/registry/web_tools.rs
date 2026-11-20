use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::browser::{BrowserController, BrowserEngine, BrowserMode};
use crate::tools::param::*;
use crate::tools::web;
use serde_json::json;
use std::path::Path;
use std::str::FromStr;

fn mode_prop() -> serde_json::Value {
    json!({
        "type": "string",
        "enum": ["headless", "gui"],
        "description": "Browser execution mode: 'headless' for background automated QA/verification during builds; 'gui' when the user asks to run, open, launch, preview, or view the website/app (opens a visible window preserved across turns)."
    })
}

fn engine_prop() -> serde_json::Value {
    json!({
        "type": "string",
        "enum": ["minibrowse", "chrome", "firefox", "safari"],
        "description": "Optional browser engine selector. Omit to use the active browser session or MiniBrowse (primary default); pass 'minibrowse', 'chrome', 'firefox', or 'safari' when testing in a specific browser."
    })
}

fn space_prop() -> serde_json::Value {
    json!({
        "type": "string",
        "description": "Optional isolated background Space name (e.g. 'checkout-qa', 'docs-scout'). Each named Space runs in its own isolated BrowserContext + tab + ARIA ref table without disturbing the user's foreground GUI tab or stealing focus."
    })
}

fn proxy_prop() -> serde_json::Value {
    json!({
        "type": "string",
        "description": "Optional HTTP/SOCKS proxy URI for an isolated Space (e.g. 'http://127.0.0.1:8080' or 'socks5://127.0.0.1:1080')."
    })
}

/// Identifies niche/redundant Web wrapper schemas deferred from automatic `ToolCategory::Web` bulk-loading
/// in `ToolFilterMode::Dynamic` (`browser-harness` minimal-footprint pattern). They remain registered in
/// `ToolRegistry` (`TOTAL_TOOL_COUNT` invariant), listed in `<available_deferred_tools>`, and on-demand selectable via `activate_tools(category="select:...")`.
pub fn is_deferred_auxiliary_web_tool(name: &str) -> bool {
    matches!(
        name,
        "browser_badges"
            | "browser_aura"
            | "browser_metrics"
            | "browser_scroll_horizontal"
            | "browser_debug_logs"
            | "browser_check_injection"
            | "crawl_sitemap"
            | "search_crawled_docs"
    )
}

pub fn get_schemas() -> Vec<ToolSchema> {
    let mode = mode_prop();
    let engine = engine_prop();
    let space = space_prop();
    let proxy = proxy_prop();
    vec![
        ToolSchema {
            name: "webfetch".to_string(),
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
            description: "Navigate to a web page or local development server (e.g. http://localhost:3000) using MiniBrowse as primary default (or Chrome, Firefox, Safari when requested via `engine` or configured) and extract an interactive ARIA accessibility tree with numbered element references (@v1:e1, @v1:e2). Pass mode='gui' when the user asks to run, open, launch, or view the website so a persistent visible window stays open across turns; pass mode='headless' for background build verification.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The web URL or localhost address to navigate to"
                    },
                    "engine": engine,
                    "space": space,
                    "proxy": proxy,
                    "mode": mode
                },
                "required": ["url"]
            }),
        },
        ToolSchema {
            name: "browser_snapshot".to_string(),
            description: "Capture an accessible ARIA DOM snapshot of the active browser page, a given URL, or raw HTML string to inspect interactive UI components.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL of the page (optional if already navigated)"
                    },
                    "html": {
                        "type": "string",
                        "description": "Raw HTML string to parse into accessibility tree (optional)"
                    },
                    "engine": engine,
                    "space": space,
                    "mode": mode
                },
                "required": []
            }),
        },
        ToolSchema {
            name: "browser_click".to_string(),
            description: "Click an interactive element by ARIA reference (@v1:e1, e1, '1'), CSS selector, or viewport `(x, y)` coordinates (piercing canvas, cross-origin iframes, and closed Shadow DOM) and return the updated accessibility snapshot.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "ARIA element reference identifier to click (e.g. '@v1:e1', 'e1', '@1', or '1')"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector target (alternative to 'ref')"
                    },
                    "x": {
                        "type": "number",
                        "description": "Viewport X coordinate in CSS pixels for direct compositor click"
                    },
                    "y": {
                        "type": "number",
                        "description": "Viewport Y coordinate in CSS pixels for direct compositor click"
                    },
                    "engine": engine,
                    "space": space,
                    "mode": mode
                },
                "required": []
            }),
        },
        ToolSchema {
            name: "browser_fill".to_string(),
            description: "Type text into an input, textarea, or contenteditable element by reference (@v1:e2, e2, '2') or CSS selector. Pass '$ENV:VAR_NAME' or approval-gated `secret_cmd` ('pass show ...', 'op read ...', 'bw get password ...') to inject secrets without exposing values to LLM context.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "ARIA element reference identifier to fill (e.g. '@v1:e2', 'e2', or '2')"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector target (alternative to 'ref')"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text string (or '$ENV:VAR_NAME' handle) to type into the form element"
                    },
                    "secret_cmd": {
                        "type": "string",
                        "description": "Read-only password CLI ('pass show ...', 'op read ...', 'bw get password ...', 'secret-tool lookup ...', 'gopass show ...', 'rbw get ...'); requires user approval and masks output"
                    },
                    "engine": engine,
                    "space": space,
                    "mode": mode
                },
                "required": []
            }),
        },
        ToolSchema {
            name: "browser_scroll".to_string(),
            description: "Scroll the browser viewport or a specific scrollable container in a given direction ('up', 'down', 'top', 'bottom'). Supports optional `selector`, `ref`, and `pixels` to target inner overflow containers.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down", "top", "bottom"],
                        "description": "Scroll direction (default: 'down')"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector of a specific scrollable container to scroll"
                    },
                    "ref": {
                        "type": "string",
                        "description": "Optional ARIA element reference (@v1:e3) of a scrollable container to scroll"
                    },
                    "pixels": {
                        "type": "integer",
                        "description": "Optional scroll distance in pixels when scrolling a container (default: 450)"
                    },
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_hover".to_string(),
            description: "Simulate hovering over an interactive element identified by its ARIA reference (@v1:e1) or CSS selector to trigger hover dropdowns, navigation menus, and tooltips.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "ARIA element reference (e.g. '@v1:e1')"
                    },
                    "selector": {
                        "type": "string",
                        "description": "CSS selector to hover over (alternative to 'ref')"
                    },
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_select_option".to_string(),
            description: "Select an option in a <select> dropdown element by its visible option text or value and dispatch input/change events.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "ARIA element reference for the <select> element"
                    },
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the <select> element (alternative to 'ref')"
                    },
                    "option_text": {
                        "type": "string",
                        "description": "Visible text or value of the option to select"
                    },
                    "mode": mode
                },
                "required": ["option_text"]
            }),
        },
        ToolSchema {
            name: "browser_scroll_horizontal".to_string(),
            description: "Scroll horizontally ('left' or 'right') across wide web pages or within specific scrollable table/container elements.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Horizontal scroll direction (default: 'right')"
                    },
                    "pixels": {
                        "type": "integer",
                        "description": "Number of horizontal pixels to scroll (default: 400)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector of the scrollable container"
                    },
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_badges".to_string(),
            description: "Inject or clear high-contrast colored visual badge overlays ([1], [2], [3]) on interactive elements for visual verification in screenshots.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["inject", "clear"],
                        "description": "Action: 'inject' to show badges, 'clear' to remove badges"
                    },
                    "mode": mode
                },
                "required": ["action"]
            }),
        },
        ToolSchema {
            name: "browser_tabs".to_string(),
            description: "Manage browser tabs, attach to a running local Chrome/Brave (`attach_local_browser` via `DevToolsActivePort`), and manage isolated background Spaces (`list`, `open_or_reuse`, `spaces`, `close_space`, `create`, `switch`, `close`).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "open_or_reuse", "attach_local_browser", "spaces", "close_space", "create", "switch", "close"],
                        "description": "Tab / Space / attach operation"
                    },
                    "url": {
                        "type": "string",
                        "description": "Target URL when creating or reusing a tab (defaults to 'about:blank')"
                    },
                    "target_id": {
                        "type": "string",
                        "description": "Target ID when switching/closing a tab, or Space name for close_space"
                    },
                    "engine": engine,
                    "space": space,
                    "proxy": proxy,
                    "mode": mode
                },
                "required": ["action"]
            }),
        },
        ToolSchema {
            name: "browser_metrics".to_string(),
            description: "Retrieve viewport dimensions and scroll metrics (pixels above/below, pages above/below, scroll positions) to understand page depth.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "mode": mode }
            }),
        },
        ToolSchema {
            name: "browser_aura".to_string(),
            description: "Toggle the Alibaba Page-Agent visual simulator aura in the browser (luminous 4-corner glow border, animated AI agent cursor, click ripple wave, and floating status pill) to visually monitor agent execution in real-time.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "enable": {
                        "type": "boolean",
                        "description": "Enable (true) or disable (false) the simulator visual aura (default: true)"
                    },
                    "status": {
                        "type": "string",
                        "description": "Initial status text to display on the floating status pill"
                    },
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_debug_logs".to_string(),
            description: "Inspect live browser runtime diagnostics including console logs (errors/warnings), uncaught JS exceptions, and failed HTTP network requests (4xx/5xx). Always use element references from the most recent browser tool response; older refs may be stale.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "mode": mode }
            }),
        },
        ToolSchema {
            name: "browser_eval".to_string(),
            description: "Evaluate JavaScript in the browser context (supports async/Promises and the built-in single-pass `window.mb` / `window.ego` Agent SDK: `mb.snapshot()`, `mb.click(1)`, `mb.fill(2, val)`, `await mb.waitFor(selOrText)`, `mb.extractTables()`, `mb.extractLinks()`, `await mb.paginate({ next, maxPages })`).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "The JavaScript expression or async workflow snippet to execute"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional execution timeout in seconds (1-60, default 25) for long-running async scripts or animation probes"
                    },
                    "engine": engine,
                    "space": space,
                    "mode": mode
                },
                "required": ["script"]
            }),
        },
        ToolSchema {
            name: "browser_screenshot".to_string(),
            description: "Capture a viewport screenshot of the currently active browser page as a PNG image and save it to `.minicode/screenshots/`. Supports optional `selector` (or `scroll_y`) to scroll a target section into view first, `if_changed: true` (perceptual dHash gate that skips duplicate image tokens if visually unchanged), and `annotate: true` (draws Set-of-Mark `[e1]`, `[e2]` bounding-box callouts matching ARIA refs without mutating DOM layout).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional filename or relative path inside `.minicode/screenshots/` to save the screenshot image (e.g. `hero.png` or `.minicode/screenshots/hero.png`)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector (e.g. `#swarm`, `#pricing`, `footer`) to scroll into view (automatically accounting for fixed headers) before capturing the screenshot"
                    },
                    "scroll_y": {
                        "type": "integer",
                        "description": "Optional vertical pixel offset (`window.scrollY`) to scroll to before capturing the screenshot"
                    },
                    "if_changed": {
                        "type": "boolean",
                        "description": "Skip capturing/attaching screenshot if perceptual dHash shows viewport is visually unchanged since last screenshot (default: false)"
                    },
                    "annotate": {
                        "type": "boolean",
                        "description": "Overlay crisp Set-of-Mark `[e1]`, `[e2]` bounding boxes matching ARIA refs directly onto the PNG buffer (default: false)"
                    },
                    "engine": engine,
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_batch".to_string(),
            description: "Execute an atomic pipeline of multiple browser actions sequentially in a single turn without round-tripping to LLM between each step. Supports navigate, click, fill, scroll, screenshot, wait_for_selector, wait_for_network_idle, delay, eval_js, and assert_text.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "actions": {
                        "type": "array",
                        "description": "Ordered array of browser action objects to execute sequentially (e.g. [{\"action\": \"navigate\", \"url\": \"...\"}, {\"action\": \"scroll\", \"direction\": \"down\"}, {\"action\": \"screenshot\", \"path\": \".minicode/screenshots/hero.png\"}, {\"action\": \"click\", \"ref\": \"@v1:e3\"}, {\"action\": \"eval_js\", \"script\": \"return document.title\"}])",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": [
                                        "navigate",
                                        "click",
                                        "hover",
                                        "fill",
                                        "select_option",
                                        "press_key",
                                        "drag",
                                        "set_checked",
                                        "scroll",
                                        "scroll_to",
                                        "scroll_horizontal",
                                        "scroll_container",
                                        "wait_for_selector",
                                        "wait_for_text",
                                        "wait_for_network_idle",
                                        "wait_delay",
                                        "delay",
                                        "eval_js",
                                        "evaluate_js",
                                        "assert_text",
                                        "assert_selector",
                                        "assert_url",
                                        "emulate",
                                        "screenshot"
                                    ],
                                    "description": "Browser operation step name (discriminator key is 'action')"
                                },
                                "url": { "type": "string" },
                                "url_contains": { "type": "string" },
                                "ref": { "type": "string", "description": "ARIA snapshot element reference (e.g. '@v1:e3')" },
                                "selector": { "type": "string", "description": "CSS selector target" },
                                "source_ref": { "type": "string" },
                                "source_selector": { "type": "string" },
                                "target_ref": { "type": "string" },
                                "target_selector": { "type": "string" },
                                "key": { "type": "string", "description": "Keyboard key name for press_key (e.g. 'Enter', 'Escape', 'Tab', 'ArrowDown')" },
                                "checked": { "type": "boolean" },
                                "dx": { "type": "integer" },
                                "dy": { "type": "integer" },
                                "text": { "type": "string" },
                                "option_text": { "type": "string" },
                                "direction": { "type": "string" },
                                "pixels": { "type": "integer" },
                                "x": { "type": "number" },
                                "y": { "type": "number" },
                                "path": { "type": "string" },
                                "script": { "type": "string", "description": "JavaScript source for eval_js / evaluate_js" },
                                "timeout_ms": { "type": "integer" },
                                "ms": { "type": "integer" },
                                "seconds": { "type": "number" },
                                "viewport": { "type": "string" },
                                "width": { "type": "integer" },
                                "height": { "type": "integer" }
                            },
                            "required": ["action"]
                        }
                    },
                    "engine": engine,
                    "mode": mode
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
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_debug_bundle".to_string(),
            description: "Aggregate live browser diagnostics into a unified report: uncaught JavaScript exceptions, console logs, failed HTTP network requests (4xx/5xx / blocked requests), and current active page ARIA tree.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "engine": engine,
                    "space": space,
                    "mode": mode
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
                    "engine": engine,
                    "space": space,
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_inspect_dom".to_string(),
            description: "Perform deep in-page DOM grounding inspection. Pierces Shadow DOM to discover custom Web Components, computes live bounding box geometry (X, Y, W, H), checks element visibility, and extracts precise CSS selectors.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "engine": engine,
                    "space": space,
                    "mode": mode
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
                    "engine": engine,
                    "space": space,
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_state".to_string(),
            description: "Manage AEAD-encrypted browser session profiles and semantic session memory in .minicode/browser_state/ (`save`, `restore`, `list`, `describe`, `import_cookies`, `delete`).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["save", "restore", "list", "describe", "import_cookies", "delete"],
                        "description": "Session state action: 'save', 'restore', 'list', 'describe', 'import_cookies', or 'delete'"
                    },
                    "profile": {
                        "type": "string",
                        "description": "Profile name (required for save, restore, describe, import_cookies, delete; omit for list)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Semantic memory description of this session (e.g. 'Admin logged into staging :3000')"
                    },
                    "cookie_file": {
                        "type": "string",
                        "description": "Workspace file path to Netscape cookies.txt or JSON cookie export for import_cookies"
                    },
                    "url": {
                        "type": "string",
                        "description": "Optional seed URL hint when importing cookies"
                    },
                    "space": space,
                    "mode": mode
                },
                "required": ["action"]
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
                    "mode": mode
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
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_interact".to_string(),
            description: "Execute advanced PageAgent + MiniBrowse interactions, structured extraction, HITL handoff, native Skill Forge, raw CDP escape hatch, element search, and action replay: keyboard ('press_key'), 'drag', 'check'/'uncheck', 'double_click'/'right_click', 'wait_for_text', history ('go_back'/'go_forward'), 'upload_file', 'configure_dialog', 'api_traffic', 'extract_tables', 'handoff_to_user' / 'release_handoff', 'forge_skill_blueprint', 'raw_cdp' (direct CDP `Domain.method` escape hatch), 'find' (compact ARIA element filter), 'set_adblock', 'suspend_idle_tabs', 'record_start' / 'record_stop' (HTML timeline replay scrubber), 'visual_diff', 'network_waterfall', and 'storage_inspect'.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "press_key",
                            "drag",
                            "check",
                            "uncheck",
                            "double_click",
                            "right_click",
                            "wait_for_text",
                            "go_back",
                            "go_forward",
                            "upload_file",
                            "configure_dialog",
                            "api_traffic",
                            "extract_tables",
                            "handoff_to_user",
                            "release_handoff",
                            "forge_skill_blueprint",
                            "raw_cdp",
                            "find",
                            "set_adblock",
                            "suspend_idle_tabs",
                            "media_diet",
                            "set_webgl",
                            "freeze_gui",
                            "thaw_gui",
                            "set_content",
                            "memory_status",
                            "trim_memory",
                            "record_start",
                            "record_stop",
                            "visual_diff",
                            "network_waterfall",
                            "storage_inspect"
                        ],
                        "description": "Interaction, extraction, handoff, CDP, or replay operation"
                    },
                    "ref": {
                        "type": "string",
                        "description": "Optional ARIA element reference (e.g. '@v1:e3')"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector target"
                    },
                    "key": {
                        "type": "string",
                        "description": "Key name for press_key (e.g. 'Enter', 'Escape', 'Tab', 'ArrowDown', 'k')"
                    },
                    "modifiers": {
                        "type": "integer",
                        "description": "Optional modifier bitmask for press_key (1=Alt, 2=Ctrl, 4=Meta, 8=Shift)"
                    },
                    "source_ref": { "type": "string" },
                    "source_selector": { "type": "string" },
                    "target_ref": { "type": "string" },
                    "target_selector": { "type": "string" },
                    "dx": { "type": "integer" },
                    "dy": { "type": "integer" },
                    "text": { "type": "string", "description": "Target text for wait_for_text or prompt response" },
                    "query": { "type": "string", "description": "Filter keyword for action='find'" },
                    "cdp_method": { "type": "string", "description": "CDP Domain.method name for action='raw_cdp' (e.g. 'Performance.getMetrics')" },
                    "cdp_params": { "type": "object", "description": "JSON parameters object for action='raw_cdp'" },
                    "enabled": { "type": "boolean", "description": "Toggle flag for action='set_adblock', 'media_diet', or 'set_webgl'" },
                    "html": { "type": "string", "description": "Raw HTML document string for action='set_content'" },
                    "audio_only": { "type": "boolean", "description": "Suspend <video> paint/upscaling while keeping <audio> playing for action='media_diet'" },
                    "hard": { "type": "boolean", "description": "Recycle idle renderer WebProcess if above cap for action='trim_memory'" },
                    "idle_secs": { "type": "integer", "description": "Idle threshold in seconds for action='suspend_idle_tabs' (default: 60)" },
                    "name": { "type": "string", "description": "Recording slug for action='record_start'" },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "File paths for upload_file action"
                    },
                    "accept": { "type": "boolean", "description": "Accept or dismiss confirm()/prompt() dialogs in configure_dialog" },
                    "prompt_text": { "type": "string", "description": "Default return string for window.prompt() in configure_dialog" },
                    "filter": { "type": "string", "description": "Optional wildcard URL filter for api_traffic (e.g. '*/api/*')" },
                    "only_errors": { "type": "boolean", "description": "Filter api_traffic to 4xx/5xx/network failures only" },
                    "baseline": { "type": "string", "description": "Baseline name for visual_diff (default: 'default')" },
                    "reason": { "type": "string", "description": "Reason shown to the user during handoff_to_user" },
                    "profile": { "type": "string", "description": "Optional browser_state profile to auto-save upon release_handoff" },
                    "keep_gui": { "type": "boolean", "description": "Whether to keep GUI window visible after release_handoff (default: true)" },
                    "skill_name": { "type": "string", "description": "Target skill slug for forge_skill_blueprint (e.g. 'github-issues-api')" },
                    "save_skill": { "type": "boolean", "description": "Persist forged SKILL.md directly to .minicode/skills/<skill_name>/SKILL.md (default: false)" },
                    "space": space,
                    "save_only": { "type": "boolean", "description": "When true in visual_diff, saves current viewport as baseline" },
                    "url_a": { "type": "string", "description": "Optional first URL to compare in visual_diff (e.g. production URL)" },
                    "url_b": { "type": "string", "description": "Optional second URL to compare in visual_diff (e.g. localhost URL)" },
                    "timeout_ms": { "type": "integer" },
                    "engine": engine,
                    "mode": mode
                },
                "required": ["action"]
            }),
        },
        ToolSchema {
            name: "browser_research".to_string(),
            description: "Render a live documentation/research web page in MiniBrowse (blocking ad/tracker scripts and executing JS/SPAs in unmapped lean mode) and distill clean Prune4Web Markdown (`format='markdown'`) or a compact hierarchical semantic tree (`format='semantic_tree'`) fenced inside untrusted-content nonce boundaries.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Optional URL to navigate to before extracting research (uses active page if omitted)"
                    },
                    "query": {
                        "type": "string",
                        "description": "Optional search keywords to score and filter relevant headings, paragraphs, and code blocks"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["markdown", "semantic_tree"],
                        "description": "Output serialization format: 'markdown' (default prose + GFM tables/code) or 'semantic_tree' (compact hierarchical UI tree)"
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Maximum character budget (default: 12000)"
                    },
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_web_diagnose".to_string(),
            description: "Run MiniBrowse deep root-cause Web diagnostics: pierces Vite (<vite-error-overlay>) and Next.js (<nextjs-portal>) Shadow DOM error overlays, identifies overflowing elements with exact computed CSS properties, audits WCAG 2.1 contrast ratios, checks fixed-header H1 occlusion, and lists failed 4xx/5xx fetch calls.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Optional URL to navigate to before running diagnostics (uses active page if omitted)"
                    },
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "flutter_debug".to_string(),
            description: "Inspect and debug Flutter Web & Dart VM Service applications via MiniBrowse: auto-activates <flt-semantics-placeholder>, computes Differential UI Trees, maps rendered Flutter widgets to exact .dart:line:column source coordinates, triggers sub-second reloadSources hot reload, and toggles debugPaint / dark-light brightness.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["status", "widget_tree", "hot_reload", "debug_paint", "brightness", "time_dilation"],
                        "description": "Flutter inspection/control operation (default: 'status')"
                    },
                    "url": {
                        "type": "string",
                        "description": "Optional Flutter Web URL (e.g. 'http://127.0.0.1:8080') to open in MiniBrowse"
                    },
                    "vm_url": {
                        "type": "string",
                        "description": "Optional Dart VM Service URL or port (e.g. 'http://127.0.0.1:8181/abc=/' or '8181') for .dart source-line widget tree and hot_reload"
                    },
                    "query": {
                        "type": "string",
                        "description": "Optional filter keyword for Flutter semantics elements"
                    },
                    "enabled": {
                        "type": "boolean",
                        "description": "Boolean toggle for debug_paint (true/false) or brightness (true=dark, false=light)"
                    },
                    "mode": mode
                }
            }),
        },
        ToolSchema {
            name: "browser_close".to_string(),
            description: "Close and terminate active browser session(s) (MiniBrowse, Chrome, Firefox, Safari, or Obscura) and reap all associated browser processes. Preserves GUI browser windows for user viewing unless force=true is specified.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "engine": {
                        "type": "string",
                        "enum": ["all", "minibrowse", "chrome", "firefox", "safari", "obscura"],
                        "description": "Optional specific browser engine to close (defaults to 'all')"
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Force shutdown even if browser was opened in GUI mode for user inspection (default: false)"
                    }
                }
            }),
        },
    ]
}

async fn resolve_browser_mode(args: &serde_json::Value) -> BrowserMode {
    if let Some(mode_str) = opt_str(args, "mode") {
        return BrowserMode::from_str(mode_str).unwrap_or(BrowserMode::Headless);
    }
    crate::tools::browser::BrowserManager::live_engine_mode()
        .await
        .unwrap_or(BrowserMode::Headless)
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    let space = opt_str(args, "space").map(str::to_string);
    let proxy = opt_str(args, "proxy").map(str::to_string);
    let engine_raw = opt_str(args, "engine")
        .or_else(|| opt_str(args, "browser"))
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto"));
    let requested_engine = if tool_name == "browser_close" {
        None
    } else if let Some(raw) = engine_raw {
        match BrowserEngine::from_str(raw) {
            Ok(eng) => Some(eng),
            Err(reason) => {
                return Some(Err(crate::error::ToolError::InvalidArguments {
                    name: tool_name.to_string(),
                    reason,
                }
                .into()));
            }
        }
    } else {
        None
    };
    crate::tools::browser::with_browser_scope(
        space,
        proxy,
        requested_engine,
        dispatch_known(tool_name, args, workspace_root),
    )
    .await
    .transpose()
}

async fn dispatch_known(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Result<Option<String>> {
    let out = match tool_name {
        "browser_close" => {
            let force = opt_bool(args, "force", false);
            let target_eng = opt_str(args, "engine")
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"))
                .map(|s| {
                    BrowserEngine::from_str(s).map_err(|reason| {
                        crate::error::ToolError::InvalidArguments {
                            name: "browser_close".to_string(),
                            reason,
                        }
                    })
                })
                .transpose()?;
            let stopped =
                crate::tools::browser::BrowserManager::shutdown_engine_opt(target_eng, force)
                    .await?;
            let is_running = crate::tools::browser::BrowserManager::is_live_engine_running().await;
            match (stopped, is_running) {
                (true, _) => {
                    "✔ Browser closed successfully. Requested browser process(es) terminated."
                        .to_string()
                }
                (false, true) => {
                    "ℹ Active GUI browser window preserved for user viewing. To terminate, pass force=true or use minitask(action=\"stop\", id=\"browser\").".to_string()
                }
                (false, false) => "ℹ No active browser session was running.".to_string(),
            }
        }
        "webfetch" => {
            let url = require_str(args, "url", "webfetch")?;
            let query_opt = opt_str(args, "query");
            web::webfetch(url, query_opt).await?
        }
        "search_web" => {
            let query = require_str(args, "query", "search_web")?;
            let max_results = opt_usize(args, "max_results", 5);
            crate::tools::web_search::WebSearchService::search(query, max_results).await?
        }
        "browser_navigate" => {
            let url = require_str(args, "url", "browser_navigate")?;
            let mode = resolve_browser_mode(args).await;
            let snapshot =
                BrowserController::navigate_and_snapshot(url, mode, workspace_root).await?;
            BrowserController::format_snapshot_report(&snapshot)
        }
        "browser_snapshot" => {
            let url_opt = opt_str(args, "url");
            let mode = resolve_browser_mode(args).await;
            let snapshot = if let Some(html) = opt_str(args, "html") {
                let page_url = url_opt.unwrap_or("http://localhost");
                BrowserController::parse_html_to_aria_snapshot(page_url, html)
            } else {
                BrowserController::snapshot_active_or_navigate(url_opt, mode, workspace_root)
                    .await?
            };
            BrowserController::format_snapshot_report(&snapshot)
        }
        "browser_click" => {
            let mode = resolve_browser_mode(args).await;
            let x_opt = args
                .get("x")
                .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)));
            let y_opt = args
                .get("y")
                .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)));
            if let Some(target_ref) = opt_str(args, "ref") {
                BrowserController::click_and_snapshot(target_ref, mode, workspace_root).await?
            } else if let Some(sel) = opt_str(args, "selector") {
                let steps = vec![crate::tools::browser::BatchStep::Click {
                    target_ref: None,
                    selector: Some(sel.to_string()),
                    x: None,
                    y: None,
                }];
                BrowserController::execute_batch(&steps, mode, workspace_root).await?
            } else if let (Some(x), Some(y)) = (x_opt, y_opt) {
                BrowserController::click_xy_and_snapshot(x, y, mode, workspace_root).await?
            } else {
                return Err(crate::error::ToolError::InvalidArguments {
                    name: "browser_click".to_string(),
                    reason: "Requires 'ref', 'selector', or '(x, y)' viewport coordinates"
                        .to_string(),
                }
                .into());
            }
        }
        "browser_fill" => {
            let mode = resolve_browser_mode(args).await;
            if let Some(secret_cmd) = opt_str(args, "secret_cmd").filter(|s| !s.trim().is_empty()) {
                BrowserController::fill_with_secret_cmd_and_snapshot(
                    opt_str(args, "ref"),
                    opt_str(args, "selector"),
                    secret_cmd,
                    mode,
                    workspace_root,
                )
                .await?
            } else {
                let text = opt_str(args, "text")
                    .or_else(|| opt_str(args, "value"))
                    .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                        name: "browser_fill".to_string(),
                        reason: "Missing required argument 'text' (or 'secret_cmd')".to_string(),
                    })?;
                if let Some(target_ref) = opt_str(args, "ref") {
                    BrowserController::fill_and_snapshot(target_ref, text, mode, workspace_root)
                        .await?
                } else if let Some(sel) = opt_str(args, "selector") {
                    let steps = vec![crate::tools::browser::BatchStep::Fill {
                        target_ref: None,
                        selector: Some(sel.to_string()),
                        text: text.to_string(),
                    }];
                    BrowserController::execute_batch(&steps, mode, workspace_root).await?
                } else {
                    return Err(crate::error::ToolError::InvalidArguments {
                        name: "browser_fill".to_string(),
                        reason: "Requires either 'ref' or 'selector'".to_string(),
                    }
                    .into());
                }
            }
        }
        "browser_scroll" => {
            let direction = opt_str(args, "direction").unwrap_or("down");
            let selector = opt_str(args, "selector");
            let target_ref = opt_str(args, "ref");
            let pixels = args
                .get("pixels")
                .and_then(|v| v.as_i64())
                .map(|p| p as i32);
            let mode = resolve_browser_mode(args).await;
            if selector.is_some() || target_ref.is_some() {
                BrowserController::scroll_container(
                    selector,
                    target_ref,
                    pixels,
                    direction,
                    mode,
                    workspace_root,
                )
                .await?
            } else {
                BrowserController::scroll(direction, mode, workspace_root).await?
            }
        }
        "browser_hover" => {
            let mode = resolve_browser_mode(args).await;
            if let Some(r) = opt_str(args, "ref") {
                BrowserController::hover_and_snapshot(r, mode, workspace_root).await?
            } else if let Some(sel) = opt_str(args, "selector") {
                let steps = vec![crate::tools::browser::BatchStep::Hover {
                    target_ref: None,
                    selector: Some(sel.to_string()),
                }];
                BrowserController::execute_batch(&steps, mode, workspace_root).await?
            } else {
                return Err(crate::error::ToolError::InvalidArguments {
                    name: "browser_hover".to_string(),
                    reason: "Requires either 'ref' or 'selector'".to_string(),
                }
                .into());
            }
        }
        "browser_select_option" => {
            let mode = resolve_browser_mode(args).await;
            let option_text = opt_str(args, "option_text")
                .or_else(|| opt_str(args, "option"))
                .or_else(|| opt_str(args, "value"))
                .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                    name: "browser_select_option".to_string(),
                    reason: "Missing required argument 'option_text' (or 'option'/'value')"
                        .to_string(),
                })?;
            if let Some(r) = opt_str(args, "ref") {
                BrowserController::select_option_and_snapshot(r, option_text, mode, workspace_root)
                    .await?
            } else if let Some(sel) = opt_str(args, "selector") {
                let steps = vec![crate::tools::browser::BatchStep::SelectOption {
                    target_ref: None,
                    selector: Some(sel.to_string()),
                    option_text: option_text.to_string(),
                }];
                BrowserController::execute_batch(&steps, mode, workspace_root).await?
            } else {
                return Err(crate::error::ToolError::InvalidArguments {
                    name: "browser_select_option".to_string(),
                    reason: "Requires either 'ref' or 'selector'".to_string(),
                }
                .into());
            }
        }
        "browser_scroll_horizontal" => {
            let direction = opt_str(args, "direction").unwrap_or("right");
            let pixels = opt_u64(args, "pixels").map(|p| p as i32);
            let selector = opt_str(args, "selector");
            let mode = resolve_browser_mode(args).await;
            BrowserController::scroll_horizontally(
                direction,
                pixels,
                selector,
                mode,
                workspace_root,
            )
            .await?
        }
        "browser_badges" => {
            let action = opt_str(args, "action").unwrap_or("inject");
            let mode = resolve_browser_mode(args).await;
            let enable = action.eq_ignore_ascii_case("inject")
                || action.eq_ignore_ascii_case("show")
                || action.eq_ignore_ascii_case("enable");
            BrowserController::toggle_visual_badges(enable, mode, workspace_root).await?
        }
        "browser_tabs" => {
            let action = require_str(args, "action", "browser_tabs")?;
            let url = opt_str(args, "url");
            let target_id = opt_str(args, "target_id");
            let mode = resolve_browser_mode(args).await;
            BrowserController::manage_tabs(action, url, target_id, mode, workspace_root).await?
        }
        "browser_metrics" => {
            let mode = resolve_browser_mode(args).await;
            let metrics: crate::tools::browser::PageMetrics =
                BrowserController::get_page_metrics(mode, workspace_root).await?;
            let json_res = serde_json::to_string_pretty(&metrics).unwrap_or_default();
            format!("### Browser Page Metrics\n```json\n{}\n```", json_res)
        }
        "browser_debug_logs" => {
            let mode = resolve_browser_mode(args).await;
            BrowserController::get_debug_logs(mode, workspace_root).await?
        }
        "browser_eval" => {
            let script = require_str(args, "script", "browser_eval")?;
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)));
            let mode = resolve_browser_mode(args).await;
            BrowserController::evaluate_js_with_timeout(script, mode, workspace_root, timeout_secs)
                .await?
        }
        "browser_screenshot" => {
            let path_opt = opt_str(args, "path");
            let selector_opt = opt_str(args, "selector").or_else(|| opt_str(args, "scroll_to"));
            let scroll_y_opt = args
                .get("scroll_y")
                .or_else(|| args.get("y"))
                .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
                .map(|v| v as i32);
            let if_changed = opt_bool(args, "if_changed", false);
            let annotate = opt_bool(args, "annotate", false);
            let mode = resolve_browser_mode(args).await;
            BrowserController::take_screenshot_with_options(
                mode,
                workspace_root,
                path_opt,
                selector_opt,
                scroll_y_opt,
                if_changed,
                annotate,
            )
            .await?
        }
        "browser_batch" => {
            let mode = resolve_browser_mode(args).await;
            let actions_val =
                args.get("actions")
                    .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                        name: "browser_batch".to_string(),
                        reason: "Missing required 'actions' array".to_string(),
                    })?;
            let steps =
                crate::tools::browser::BatchStep::parse_steps(actions_val).map_err(|e| {
                    crate::error::ToolError::InvalidArguments {
                        name: "browser_batch".to_string(),
                        reason: format!("Failed parsing batch actions: {}", e),
                    }
                })?;
            BrowserController::execute_batch(&steps, mode, workspace_root).await?
        }
        "browser_mock_route" => {
            let mode = resolve_browser_mode(args).await;
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
            .await?
        }
        "browser_debug_bundle" => {
            let mode = resolve_browser_mode(args).await;
            BrowserController::get_debug_bundle(mode, workspace_root).await?
        }
        "browser_qa_audit" => {
            let mode = resolve_browser_mode(args).await;
            let url_opt = opt_str(args, "url");
            BrowserController::run_qa_audit(url_opt, mode, workspace_root).await?
        }
        "browser_inspect_dom" => {
            let mode = resolve_browser_mode(args).await;
            BrowserController::inspect_visual_dom(mode, workspace_root).await?
        }
        "browser_aura" => {
            let mode = resolve_browser_mode(args).await;
            let enable = args.get("enable").and_then(|v| v.as_bool()).unwrap_or(true);
            let status = opt_str(args, "status");
            BrowserController::toggle_simulator_aura(enable, status, mode, workspace_root).await?
        }
        "crawl_documentation" => {
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
                report.total_pages_crawled, report.root_url, report.total_chars, report.duration_ms
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

            out
        }
        "crawl_sitemap" => {
            let url = require_str(args, "url", "crawl_sitemap")?;
            let max_links = opt_usize(args, "max_links", 20);

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(
                    crate::constants::WEB_TIMEOUT_SECS,
                ))
                .user_agent(crate::constants::WEB_USER_AGENT)
                .build()
                .unwrap_or_default();

            let entries = crate::tools::crawler::SitemapParser::fetch_sitemap(&client, url).await?;
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

            out
        }
        "search_crawled_docs" => {
            let query = require_str(args, "query", "search_crawled_docs")?;
            let limit = opt_usize(args, "limit", 5);

            let results = crate::tools::crawler::CrawlerEngine::search_cached_docs(
                workspace_root,
                query,
                limit,
            );
            if results.is_empty() {
                format!(
                    "ℹ No cached documentation matches found for `{}` in `.minicode/crawled/`.",
                    query
                )
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
                out
            }
        }
        "browser_emulate" => {
            let mode = resolve_browser_mode(args).await;
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
            .await?
        }
        "browser_state" => {
            let mode = resolve_browser_mode(args).await;
            let action = require_str(args, "action", "browser_state")?;
            let profile = opt_str(args, "profile");
            let description = opt_str(args, "description");
            let cookie_file = opt_str(args, "cookie_file");
            let url_hint = opt_str(args, "url");

            BrowserController::manage_session_state(
                action,
                profile,
                description,
                cookie_file,
                url_hint,
                mode,
                workspace_root,
            )
            .await?
        }
        "browser_pdf" => {
            let mode = resolve_browser_mode(args).await;
            if let Some(target_url) = opt_str(args, "url") {
                BrowserController::navigate_and_snapshot(target_url, mode, workspace_root).await?;
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

            BrowserController::export_pdf(custom_path, landscape, print_bg, mode, workspace_root)
                .await?
        }
        "browser_check_injection" => {
            let mode = resolve_browser_mode(args).await;
            if let Some(target_url) = opt_str(args, "url") {
                BrowserController::navigate_and_snapshot(target_url, mode, workspace_root).await?;
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }

            BrowserController::check_prompt_injections(mode, workspace_root).await?
        }
        "browser_research" => {
            let mode = resolve_browser_mode(args).await;
            let url_opt = opt_str(args, "url");
            let query_opt = opt_str(args, "query");
            let format_opt = opt_str(args, "format");
            let max_chars = opt_usize(args, "max_chars", 12_000);
            BrowserController::research_extract_with_format(
                url_opt,
                query_opt,
                max_chars,
                format_opt,
                mode,
                workspace_root,
            )
            .await?
        }
        "browser_interact" => {
            let mode = resolve_browser_mode(args).await;
            let action = require_str(args, "action", "browser_interact")?;
            BrowserController::interact_advanced(action, args, mode, workspace_root).await?
        }
        "browser_web_diagnose" => {
            let mode = resolve_browser_mode(args).await;
            let url_opt = opt_str(args, "url");
            BrowserController::diagnose_web_app(url_opt, mode, workspace_root).await?
        }
        "flutter_debug" => {
            let mode = resolve_browser_mode(args).await;
            let action = opt_str(args, "action").unwrap_or("status");
            let url_opt = opt_str(args, "url");
            let vm_url_opt = opt_str(args, "vm_url").or_else(|| opt_str(args, "vmUrl"));
            let query_opt = opt_str(args, "query");
            let enabled_opt = args.get("enabled").and_then(|v| v.as_bool());
            BrowserController::flutter_debug(
                action,
                url_opt,
                vm_url_opt,
                query_opt,
                enabled_opt,
                mode,
                workspace_root,
            )
            .await?
        }
        _ => return Ok(None),
    };
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_snapshot_schema_optional_url() {
        let tools = get_schemas();
        let snapshot_tool = tools.iter().find(|t| t.name == "browser_snapshot").unwrap();
        let required = snapshot_tool.parameters["required"].as_array().unwrap();
        assert!(
            required.is_empty(),
            "browser_snapshot url should be optional"
        );
    }

    #[test]
    fn test_browser_batch_and_mode_schemas_document_action_and_gui_mode() {
        let tools = get_schemas();
        let nav_tool = tools.iter().find(|t| t.name == "browser_navigate").unwrap();
        let mode_desc = nav_tool.parameters["properties"]["mode"]["description"]
            .as_str()
            .unwrap();
        assert!(mode_desc.contains("'gui'"));
        assert!(mode_desc.contains("preserved across turns"));

        let batch_tool = tools.iter().find(|t| t.name == "browser_batch").unwrap();
        let item_props = &batch_tool.parameters["properties"]["actions"]["items"]["properties"];
        assert!(item_props["action"]["enum"].is_array());
        assert!(item_props["script"].is_object());
        let item_req = batch_tool.parameters["properties"]["actions"]["items"]["required"]
            .as_array()
            .unwrap();
        assert!(item_req.iter().any(|v| v == "action"));
    }
}
