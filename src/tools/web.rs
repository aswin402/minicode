use crate::error::{Result, ToolError};
use scraper::{Html, Selector};

/// Fetches web documentation or articles and distills HTML into readable Markdown.
pub async fn fetch_or_browse(url: &str) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ToolError::InvalidArguments {
            name: "fetch_or_browse".to_string(),
            reason: "URL must begin with http:// or https://".to_string(),
        }
        .into());
    }

    let client = reqwest::Client::builder()
        .user_agent("minicode-agent/0.1 (Documentation Fetcher)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ToolError::CommandExec(format!("HTTP client error: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| ToolError::CommandExec(format!("Failed to fetch URL {}: {}", url, e)))?;

    let status = response.status();
    if !status.is_success() {
        return Err(
            ToolError::CommandExec(format!("HTTP {} response from {}", status, url)).into(),
        );
    }

    let html_text = response
        .text()
        .await
        .map_err(|e| ToolError::CommandExec(format!("Failed to read response body: {}", e)))?;

    let document = Html::parse_document(&html_text);

    // Extract title
    let title_selector = Selector::parse("title").ok();
    let page_title = title_selector
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| url.to_string());

    // Extract body text or article content
    let mut markdown = format!("# {}\n\n*Source: {}*\n\n", page_title.trim(), url);

    let content_selector = Selector::parse("article, main, body").ok();
    if let Some(sel) = content_selector {
        if let Some(container) = document.select(&sel).next() {
            let p_selector = Selector::parse("h1, h2, h3, h4, p, li, pre, code").ok();
            if let Some(p_sel) = p_selector {
                for element in container.select(&p_sel) {
                    let tag = element.value().name();
                    let text = element
                        .text()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .trim()
                        .to_string();
                    if text.is_empty() {
                        continue;
                    }

                    match tag {
                        "h1" => markdown.push_str(&format!("\n# {}\n\n", text)),
                        "h2" => markdown.push_str(&format!("\n## {}\n\n", text)),
                        "h3" => markdown.push_str(&format!("\n### {}\n\n", text)),
                        "h4" => markdown.push_str(&format!("\n#### {}\n\n", text)),
                        "li" => markdown.push_str(&format!("* {}\n", text)),
                        "pre" | "code" => markdown.push_str(&format!("\n```\n{}\n```\n", text)),
                        _ => markdown.push_str(&format!("{}\n\n", text)),
                    }
                }
            }
        }
    }

    // Limit to 40KB
    if markdown.len() > 40 * 1024 {
        markdown.truncate(40 * 1024);
        markdown.push_str("\n\n[... Content truncated: 40KB limit ...]");
    }

    Ok(markdown)
}
