use crate::constants::SSRF_BLOCKED_HOSTS;
use crate::error::{Result, SecurityError, ToolError};

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

/// Fetches web documentation using the 3-step Smart Markdown pipeline:
/// 1. Accept: text/markdown negotiation
/// 2. llms.txt probing
/// 3. High-fidelity HTML-to-Fit-Markdown conversion with noise pruning
pub async fn fetch_or_browse(url: &str, query: Option<&str>) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ToolError::InvalidArguments {
            name: "fetch_or_browse".to_string(),
            reason: "URL must begin with http:// or https://".to_string(),
        }
        .into());
    }

    validate_ssrf(url)?;

    crate::tools::browser::markdown::SmartMarkdownExtractor::fetch_smart_markdown(url, query).await
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
