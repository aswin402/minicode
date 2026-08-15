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
        .user_agent(crate::constants::WEB_USER_AGENT)
        .timeout(std::time::Duration::from_secs(
            crate::constants::WEB_TIMEOUT_SECS,
        ))
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

    if let Some(content_length) = response.content_length() {
        if content_length > crate::constants::MAX_WEB_RESPONSE_BYTES as u64 {
            return Err(ToolError::CommandExec(format!(
                "Response body too large ({} bytes, maximum allowed is {} bytes)",
                content_length,
                crate::constants::MAX_WEB_RESPONSE_BYTES
            ))
            .into());
        }
    }

    use futures::StreamExt;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res
            .map_err(|e| ToolError::CommandExec(format!("Failed reading response chunk: {}", e)))?;
        if bytes.len() + chunk.len() > crate::constants::MAX_WEB_RESPONSE_BYTES {
            return Err(ToolError::CommandExec(format!(
                "Response body exceeded maximum limit of {} bytes",
                crate::constants::MAX_WEB_RESPONSE_BYTES
            ))
            .into());
        }
        bytes.extend_from_slice(&chunk);
    }

    let html_text = String::from_utf8_lossy(&bytes).to_string();

    let document = Html::parse_document(&html_text);

    // Extract title
    let title_selector = match Selector::parse("title") {
        Ok(sel) => Some(sel),
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to parse title CSS selector");
            None
        }
    };
    let page_title = title_selector
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| url.to_string());

    // Extract body text or article content
    let mut markdown = format!("# {}\n\n*Source: {}*\n\n", page_title.trim(), url);

    let content_selector = match Selector::parse("article, main, body") {
        Ok(sel) => Some(sel),
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to parse content CSS selector");
            None
        }
    };
    if let Some(sel) = content_selector {
        if let Some(container) = document.select(&sel).next() {
            let p_selector = match Selector::parse("h1, h2, h3, h4, p, li, pre") {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!(error = ?e, "Failed to parse content element CSS selector");
                    None
                }
            };
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

    // Limit to max body size
    if markdown.len() > crate::constants::WEB_MAX_BODY_BYTES {
        let valid_len = markdown.floor_char_boundary(crate::constants::WEB_MAX_BODY_BYTES);
        markdown.truncate(valid_len);
        markdown.push_str("\n\n[... Content truncated: max limit reached ...]");
    }

    Ok(markdown)
}
