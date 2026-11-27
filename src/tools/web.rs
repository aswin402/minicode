use crate::constants::SSRF_BLOCKED_HOSTS;
use crate::error::{Result, SecurityError, ToolError};

/// Validates whether a URL targets a loopback, internal private network, or cloud metadata endpoint
pub fn validate_ssrf(url_str: &str) -> Result<()> {
    let parsed = url::Url::parse(url_str).map_err(|e| ToolError::InvalidArguments {
        name: "webfetch".to_string(),
        reason: format!("Invalid URL '{}': {}", url_str, e),
    })?;

    let host_str = match parsed.host_str() {
        Some(h) => h.to_lowercase(),
        None => {
            return Err(ToolError::InvalidArguments {
                name: "webfetch".to_string(),
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

    // url keeps brackets on IPv6 literals ("[::1]"); strip them for parsing.
    let ip_literal = host_str.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = ip_literal.parse::<std::net::IpAddr>() {
        if is_private_ip(ip) {
            return Err(SecurityError::SsrfBlocked {
                url: url_str.to_string(),
                reason: format!("Access to private/local IP address '{}' is forbidden", ip),
            }
            .into());
        }
    }

    Ok(())
}

/// True for loopback/link-local/private/CGNAT addresses, including IPv4-mapped IPv6.
fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
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
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_private_ip(std::net::IpAddr::V4(mapped));
            }
            v6.is_loopback() || v6.is_unspecified() || ((v6.segments()[0] & 0xfe00) == 0xfc00)
        }
    }
}

/// Resolves the URL host via DNS and blocks any result landing on private
/// address space (defeats rebinding hosts like `localtest.me` → 127.0.0.1).
async fn assert_resolves_public(parsed: &url::Url) -> Result<()> {
    let host = parsed.host_str().unwrap_or_default();
    let port = parsed.port_or_known_default().unwrap_or(80);
    // Literal IPs were already checked by validate_ssrf.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }
    let addrs =
        tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| SecurityError::SsrfBlocked {
                url: parsed.to_string(),
                reason: format!("DNS resolution failed: {}", e),
            })?;
    for addr in addrs {
        if is_private_ip(addr.ip()) {
            return Err(SecurityError::SsrfBlocked {
                url: parsed.to_string(),
                reason: format!("Host resolves to private address '{}'", addr.ip()),
            }
            .into());
        }
    }
    Ok(())
}

/// Fetches web documentation or decodes safe inline `data:` URIs via the unified Smart Markdown
/// + priority-ordered `DocumentConverterRegistry` pipeline:
/// 1. Safe inline `data:` URI decoding & conversion (zero network/SSRF exposure)
/// 2. `Accept: text/markdown` negotiation + binary/structured stream conversion (PDF/OOXML/RSS/CSV/IPYNB)
/// 3. `llms.txt` probing
/// 4. High-fidelity HTML-to-Fit-Markdown distillation with SPA escalation
pub async fn webfetch(url: &str, query: Option<&str>) -> Result<String> {
    if url.starts_with("data:") {
        let (bytes, mime) = crate::context::doc_convert::decode_data_uri(url).map_err(|e| {
            ToolError::InvalidArguments {
                name: "webfetch".to_string(),
                reason: e.to_string(),
            }
        })?;
        let bounded = &bytes[..bytes.len().min(crate::constants::WEB_MAX_BODY_BYTES)];
        let md = match crate::context::doc_convert::DocumentConverterRegistry::global()
            .convert_bytes(
                bounded,
                None,
                mime.as_deref(),
                crate::context::doc_convert::ConversionPolicy::WebOrStream,
            )? {
            Some(doc) => doc.markdown,
            None => String::from_utf8_lossy(bounded).into_owned(),
        };
        let filtered = match query.map(str::trim).filter(|q| !q.is_empty()) {
            Some(q) => {
                let q_lower = q.to_ascii_lowercase();
                let matched: Vec<&str> = md
                    .lines()
                    .filter(|l| l.starts_with('#') || l.to_ascii_lowercase().contains(&q_lower))
                    .collect();
                if matched.is_empty() {
                    md
                } else {
                    matched.join("\n")
                }
            }
            None => md,
        };
        return Ok(crate::tools::browser::guard::wrap_untrusted_web_output(
            "data:uri", &filtered,
        ));
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ToolError::InvalidArguments {
            name: "webfetch".to_string(),
            reason: "URL must begin with http://, https://, or data:".to_string(),
        }
        .into());
    }

    let parsed = url::Url::parse(url).map_err(|e| ToolError::InvalidArguments {
        name: "webfetch".to_string(),
        reason: format!("Invalid URL '{}': {}", url, e),
    })?;
    validate_ssrf(url)?;
    // Post-DNS check: reject hostnames resolving into private space.
    assert_resolves_public(&parsed).await?;

    let md =
        crate::tools::browser::markdown::SmartMarkdownExtractor::fetch_smart_markdown(url, query)
            .await?;
    Ok(crate::tools::browser::guard::wrap_untrusted_web_output(
        url, &md,
    ))
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

    #[test]
    fn test_validate_ssrf_blocks_mapped_ipv6_private() {
        assert!(validate_ssrf("http://[::ffff:10.0.0.5]/secrets").is_err());
        assert!(validate_ssrf("http://[::ffff:127.0.0.1]/admin").is_err());
    }

    #[test]
    fn test_is_private_ip_covers_mapped_ipv6_and_ranges() {
        assert!(super::is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(super::is_private_ip("::ffff:10.0.0.5".parse().unwrap()));
        assert!(super::is_private_ip(
            "::ffff:169.254.169.254".parse().unwrap()
        ));
        assert!(!super::is_private_ip("93.184.216.34".parse().unwrap()));
        assert!(!super::is_private_ip(
            "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()
        ));
    }

    #[tokio::test]
    async fn test_webfetch_decodes_and_converts_data_uri_csv_and_html() {
        let csv_uri = "data:text/csv;base64,bmFtZSxwb3J0CmFwaSw4MDgwCmRiLDU0MzIK";
        let out = webfetch(csv_uri, None).await.unwrap();
        assert!(out.contains("| name | port |"));
        assert!(out.contains("| api | 8080 |"));

        let html_uri = "data:text/html,%3Ch1%3ESpec%3C%2Fh1%3E%3Cp%3EClean%20Markdown%3C%2Fp%3E";
        let out_html = webfetch(html_uri, None).await.unwrap();
        assert!(out_html.contains("Spec"));
        assert!(out_html.contains("Clean Markdown"));
    }
}
