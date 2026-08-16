use crate::constants::SSRF_BLOCKED_HOSTS;
use crate::error::{Result, SecurityError, ToolError};
use scraper::{Html, Selector};

/// Validates whether a URL targets a loopback, internal private network, or cloud metadata endpoint
pub fn validate_ssrf(url_str: &str) -> Result<()> {
    let parsed = url::Url::parse(url_str).map_err(|e| ToolError::InvalidArguments {
        name: "fetch_or_browse".to_string(),
        reason: format!("Invalid URL '{}': {}", url_str, e),
    })?;

    let host_str = match parsed.host_str() {
        Some(h) => h.to_lowercase(),
        None => {
            return Err(ToolError::InvalidArguments {
                name: "fetch_or_browse".to_string(),
                reason: "URL is missing a valid host".to_string(),
            }
            .into());
        }
    };

    for blocked in SSRF_BLOCKED_HOSTS {
        if host_str == *blocked || host_str.ends_with(&format!(".{}", blocked)) {
            return Err(SecurityError::SsrfBlocked {
                url: url_str.to_string(),
                reason: format!("Access to blocked host '{}' is forbidden", host_str),
            }
            .into());
        }
    }

    if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
        let is_private = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_broadcast()
                    || v4.is_documentation()
                    || v4.is_unspecified()
                    || (v4.octets()[0] == 10)
                    || (v4.octets()[0] == 172 && (16..=31).contains(&v4.octets()[1]))
                    || (v4.octets()[0] == 192 && v4.octets()[1] == 168)
                    || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() || v6.is_unspecified() || ((v6.segments()[0] & 0xfe00) == 0xfc00)
            }
        };

        if is_private {
            return Err(SecurityError::SsrfBlocked {
                url: url_str.to_string(),
                reason: format!("Access to private/local IP address '{}' is forbidden", ip),
            }
            .into());
        }
    }

    Ok(())
}

/// Fetches web documentation or articles and distills HTML into readable Markdown.
pub async fn fetch_or_browse(url: &str) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ToolError::InvalidArguments {
            name: "fetch_or_browse".to_string(),
            reason: "URL must begin with http:// or https://".to_string(),
        }
        .into());
    }

    validate_ssrf(url)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ssrf_blocks_localhost_and_private_ips() {
        assert!(validate_ssrf("http://localhost/admin").is_err());
        assert!(validate_ssrf("http://127.0.0.1:8080/api").is_err());
        assert!(validate_ssrf("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_ssrf("http://192.168.1.1/router").is_err());
        assert!(validate_ssrf("http://10.0.0.5/secrets").is_err());
        assert!(validate_ssrf("http://172.16.0.1/internal").is_err());
    }

    #[test]
    fn test_validate_ssrf_allows_public_urls() {
        assert!(validate_ssrf("https://example.com/docs").is_ok());
        assert!(validate_ssrf("https://docs.rs/tokio/latest/tokio/").is_ok());
    }
}
