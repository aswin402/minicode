//! Pre-network tracker/ad domain suffix blocker, cross-origin SSRF guard, resource profiles,
//! and standard browser header hygiene inspired by `obscura-net` / `obscura-ssrf`.
//!
//! Uses $O(k \log N)$ binary search over a sorted static domain-suffix table in `.rodata`
//! with zero regular expressions and zero runtime heap allocations on the fast path.

use std::net::IpAddr;

/// Resource loading profile per agent operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResourceProfile {
    /// Full CSS, image, and font loading for visual QA audits, screenshots, and GUI sessions.
    #[default]
    Full,
    /// Lean text/DOM research profile: skips image, font, audio, and video subresources.
    ResearchLean,
}

/// Standard clean WebKit Linux User-Agent for external HTTPS documentation/research fetches
/// so CDNs/WAFs do not reject requests based on an unfamiliar custom User-Agent token.
pub const CLEAN_BROWSER_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15";

/// Strictly ASCII-sorted table of dedicated ad, tracker, pixel, session-replay, and telemetry domains.
/// Verified sorted in `tests::test_tracker_suffixes_strictly_sorted`.
pub const TRACKER_DOMAIN_SUFFIXES: &[&str] = &[
    "2mdn.net",
    "adnxs.com",
    "ads-twitter.com",
    "adsafeprotected.com",
    "adsrvr.org",
    "adsystem.com",
    "amazon-adsystem.com",
    "amplitude.com",
    "analytics.tiktok.com",
    "appsflyer.com",
    "bat.bing.com",
    "branch.io",
    "braze.com",
    "bugsnag.com",
    "chartbeat.com",
    "chartbeat.net",
    "clarity.ms",
    "clicktale.net",
    "cloudflareinsights.com",
    "comscore.com",
    "connect.facebook.net",
    "criteo.com",
    "criteo.net",
    "ct.pinterest.com",
    "datadoghq-browser-agent.com",
    "demdex.net",
    "doubleclick.net",
    "everesttech.net",
    "exelator.com",
    "facebook.net",
    "fastclick.net",
    "fingerprintjs.com",
    "fpjs.io",
    "freshmarketer.com",
    "fullstory.com",
    "go-mpulse.net",
    "google-analytics.com",
    "googleadservices.com",
    "googlesyndication.com",
    "googletagmanager.com",
    "googletagservices.com",
    "heapanalytics.com",
    "hotjar.com",
    "hotjar.io",
    "hs-analytics.net",
    "hs-scripts.com",
    "hsadspixel.net",
    "inspectlet.com",
    "intercomcdn.com",
    "kissmetrics.com",
    "krxd.net",
    "liadm.com",
    "loggly.com",
    "logrocket.com",
    "logrocket.io",
    "luckyorange.com",
    "marketo.net",
    "mathtag.com",
    "matomo.cloud",
    "media.net",
    "mixpanel.com",
    "moatads.com",
    "mouseflow.com",
    "mparticle.com",
    "mxpnl.com",
    "newrelic.com",
    "nr-data.net",
    "omtrdc.net",
    "onesignal.com",
    "openx.net",
    "optimizely.com",
    "outbrain.com",
    "pardot.com",
    "parsely.com",
    "pendo.io",
    "posthog.com",
    "pubmatic.com",
    "px.ads.linkedin.com",
    "quantcast.com",
    "quantserve.com",
    "rollbar.com",
    "rubiconproject.com",
    "rudderlabs.com",
    "sc-static.net",
    "scorecardresearch.com",
    "segment.com",
    "segment.io",
    "sentry-cdn.com",
    "sharethrough.com",
    "smartadserver.com",
    "snap.licdn.com",
    "statcounter.com",
    "taboola.com",
    "tapad.com",
    "tealiumiq.com",
    "treasuredata.com",
    "turn.com",
    "visualwebsiteoptimizer.com",
    "vwo.com",
    "walkme.com",
    "yieldmo.com",
];

const NON_ESSENTIAL_MEDIA_EXTS: &[&str] = &[
    "avif", "bmp", "eot", "flac", "gif", "ico", "jpeg", "jpg", "m4a", "m4v", "mkv", "mov", "mp3",
    "mp4", "ogg", "ogv", "otf", "png", "ttf", "wav", "webm", "webp", "woff", "woff2",
];

/// Checks whether a host (already lowercase or mixed-case) matches any tracker domain suffix
/// in $O(k \log N)$ via binary search along dot boundaries.
#[must_use]
pub fn is_tracker_host(raw_host: &str) -> bool {
    let host = raw_host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    let mut rest = host.as_str();
    loop {
        if TRACKER_DOMAIN_SUFFIXES.binary_search(&rest).is_ok() {
            return true;
        }
        match rest.split_once('.') {
            Some((_, tail)) if tail.contains('.') => rest = tail,
            _ => return false,
        }
    }
}

/// Checks whether `target_url` targets a known tracker/ad/telemetry host.
#[must_use]
pub fn is_tracker_url(target_url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(target_url) else {
        return false;
    };
    parsed.host_str().is_some_and(is_tracker_host)
}

/// Checks whether `target_url` is a heavy binary media/image/font asset that can be skipped
/// in `ResourceProfile::ResearchLean`.
#[must_use]
pub fn is_non_essential_media_url(target_url: &str) -> bool {
    let path = match url::Url::parse(target_url) {
        Ok(u) => u.path().to_ascii_lowercase(),
        Err(_) => target_url
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase(),
    };
    let Some((_, ext)) = path.rsplit_once('.') else {
        return false;
    };
    NON_ESSENTIAL_MEDIA_EXTS.binary_search(&ext).is_ok()
}

/// Classifies private, loopback, link-local, unspecified, CGNAT, and cloud-metadata hosts.
#[must_use]
pub fn is_private_or_loopback_host(raw_host: &str) -> bool {
    let host = raw_host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if matches!(
        host.as_str(),
        "localhost" | "metadata.google.internal" | "metadata" | "instance-data"
    ) || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_ip(ip);
    }
    false
}

const fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
                || (o[0] == 100 && o[1] >= 64 && o[1] <= 127)
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_private_ip(IpAddr::V4(mapped));
            }
            let seg0 = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (seg0 & 0xfe00) == 0xfc00
                || (seg0 & 0xffc0) == 0xfe80
        }
    }
}

/// Blocks untrusted public origins from probing loopback, RFC1918 LAN, or cloud-metadata IPs
/// via subresources or redirects, while permitting `localhost -> localhost` local full-stack dev!
#[must_use]
pub fn is_cross_origin_ssrf(page_origin_url: &str, candidate_url: &str) -> bool {
    let (Ok(origin), Ok(target)) = (
        url::Url::parse(page_origin_url),
        url::Url::parse(candidate_url),
    ) else {
        return false;
    };
    if !matches!(origin.scheme(), "http" | "https") || !matches!(target.scheme(), "http" | "https")
    {
        return false;
    }
    let Some(origin_host) = origin.host_str() else {
        return false;
    };
    let Some(target_host) = target.host_str() else {
        return false;
    };
    !is_private_or_loopback_host(origin_host) && is_private_or_loopback_host(target_host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_suffixes_strictly_sorted() {
        for pair in TRACKER_DOMAIN_SUFFIXES.windows(2) {
            assert!(
                pair[0] < pair[1],
                "TRACKER_DOMAIN_SUFFIXES out of order: '{}' >= '{}'",
                pair[0],
                pair[1]
            );
        }
        for pair in NON_ESSENTIAL_MEDIA_EXTS.windows(2) {
            assert!(
                pair[0] < pair[1],
                "NON_ESSENTIAL_MEDIA_EXTS out of order: '{}' >= '{}'",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn test_tracker_binary_search_and_legit_blog_preservation() {
        assert!(is_tracker_url("https://www.google-analytics.com/g/collect"));
        assert!(is_tracker_url("https://stats.g.doubleclick.net/r/collect"));
        assert!(is_tracker_url("https://px.ads.linkedin.com/collect"));
        assert!(!is_tracker_url("https://engineering.linkedin.com/blog"));
        assert!(!is_tracker_url("https://docs.rs/tokio/latest/tokio/"));
    }

    #[test]
    fn test_cross_origin_ssrf_guard() {
        // Public page probing localhost or cloud metadata -> BLOCKED
        assert!(is_cross_origin_ssrf(
            "https://untrusted-docs.example.com/guide",
            "http://127.0.0.1:8080/admin"
        ));
        assert!(is_cross_origin_ssrf(
            "https://untrusted-docs.example.com/guide",
            "http://169.254.169.254/latest/meta-data/"
        ));
        // Localhost frontend calling localhost backend -> ALLOWED
        assert!(!is_cross_origin_ssrf(
            "http://localhost:3000/",
            "http://127.0.0.1:8080/api/orders"
        ));
        // Public page calling public CDN -> ALLOWED
        assert!(!is_cross_origin_ssrf(
            "https://docs.rs/tokio",
            "https://static.rust-lang.org/styles.css"
        ));
    }

    #[test]
    fn test_media_extension_detection() {
        assert!(is_non_essential_media_url(
            "https://cdn.example.com/hero.webp?v=2"
        ));
        assert!(is_non_essential_media_url(
            "https://cdn.example.com/fonts/inter.woff2"
        ));
        assert!(!is_non_essential_media_url(
            "https://cdn.example.com/app.js"
        ));
        assert!(!is_non_essential_media_url("https://cdn.example.com/docs"));
    }
}
