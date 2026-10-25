//! The Browser's URL policy, shared by the desktop's WebView and the native
//! app's: what a page may be (HTTPS, or HTTP on loopback, never with
//! credentials), what of a URL may be kept on disk, and which URL a Browser
//! opens on.

use std::net::Ipv6Addr;

use url::{Host, Url};

/// Where a Browser with nothing else to show opens.
pub const DEFAULT_URL: &str = "https://www.google.com/";
/// How long a scope's last URL is kept.
pub const LOCATION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
pub const MAX_LOCATION_URL_CHARS: usize = 2_048;

/// HTTPS anywhere, HTTP only on loopback, never with credentials.
pub fn validate_browser_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("browser URL cannot be empty".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https:///") || lower.starts_with("http:///") {
        return Err("browser URL must include a host".to_string());
    }
    let url = Url::parse(trimmed).map_err(|_| "browser URL is invalid".to_string())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("browser URLs cannot contain credentials".to_string());
    }
    match url.scheme().to_ascii_lowercase().as_str() {
        "https" => {
            match url.host_str() {
                Some(host) if !host.is_empty() => {}
                _ => return Err("HTTPS browser URL must include a host".to_string()),
            }
            Ok(url)
        }
        "http" => {
            let host = url
                .host()
                .ok_or_else(|| "HTTP browser URL must include a host".to_string())?;
            let local = match host {
                Host::Domain(host) => matches!(
                    host.to_ascii_lowercase().as_str(),
                    "localhost" | "127.0.0.1"
                ),
                Host::Ipv4(host) => host.is_loopback(),
                Host::Ipv6(host) => host == Ipv6Addr::LOCALHOST,
            };
            if local {
                Ok(url)
            } else {
                Err("HTTP browser URLs are only allowed for localhost".to_string())
            }
        }
        _ => Err("browser URL scheme must be HTTPS or local HTTP".to_string()),
    }
}

/// Durable Browser locations keep navigation identity only. Query strings,
/// fragments and credentials can carry secrets, so they never reach SQLite.
pub fn sanitize_location_url(raw: &str) -> Option<String> {
    if raw.is_empty()
        || raw.chars().count() > MAX_LOCATION_URL_CHARS
        || raw.chars().any(char::is_control)
    {
        return None;
    }
    let mut url = Url::parse(raw).ok()?;
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    let safe_url = url.to_string();
    if safe_url.chars().count() > MAX_LOCATION_URL_CHARS || safe_url.chars().any(char::is_control) {
        return None;
    }
    validate_browser_url(&safe_url).ok()?;
    Some(safe_url)
}

/// The URL a Browser opens on: the one asked for, else what the open
/// Browser already shows, else (only when restoring) the scope's last one,
/// else the default.
pub fn select_open_url(
    initial_url: Option<&str>,
    runtime_url: Option<String>,
    stored_url: Option<String>,
    restore_last_url: bool,
) -> Result<Url, String> {
    if let Some(initial_url) = initial_url {
        return validate_browser_url(initial_url);
    }
    if let Some(runtime_url) = runtime_url {
        return validate_browser_url(&runtime_url);
    }
    if restore_last_url {
        if let Some(stored_url) = stored_url.and_then(|url| sanitize_location_url(&url)) {
            return validate_browser_url(&stored_url);
        }
    }
    validate_browser_url(DEFAULT_URL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_and_local_http_without_credentials() {
        assert!(validate_browser_url("https://example.com/a").is_ok());
        assert!(validate_browser_url("http://localhost:5173/").is_ok());
        assert!(validate_browser_url("http://127.0.0.1:8000").is_ok());
        assert!(validate_browser_url("http://[::1]:3000").is_ok());
        assert!(validate_browser_url("http://example.com").is_err());
        assert!(validate_browser_url("https://user:pass@example.com").is_err());
        assert!(validate_browser_url("javascript:alert(1)").is_err());
        assert!(validate_browser_url("https:///path").is_err());
        assert!(validate_browser_url("   ").is_err());
    }

    #[test]
    fn durable_locations_lose_their_secret_parts() {
        assert_eq!(
            sanitize_location_url("https://user:secret@example.test:8443/path?q=private#fragment"),
            Some("https://example.test:8443/path".to_string())
        );
        assert_eq!(
            sanitize_location_url("http://localhost:3000/path?token=private#section"),
            Some("http://localhost:3000/path".to_string())
        );
        assert!(sanitize_location_url("https://example.test/path\n").is_none());
        assert!(sanitize_location_url("http://example.test/private").is_none());
        assert!(sanitize_location_url("javascript:alert(1)").is_none());
    }

    #[test]
    fn the_open_url_keeps_restoring_explicit() {
        let pick = |initial, runtime: Option<&str>, stored: Option<&str>, restore| {
            select_open_url(
                initial,
                runtime.map(str::to_string),
                stored.map(str::to_string),
                restore,
            )
            .unwrap()
            .to_string()
        };
        assert_eq!(
            pick(
                Some("https://initial.test/explicit"),
                Some("https://runtime.test/current"),
                Some("https://stored.test/last"),
                true
            ),
            "https://initial.test/explicit"
        );
        assert_eq!(
            pick(
                None,
                Some("https://runtime.test/current"),
                Some("https://stored.test/last"),
                true
            ),
            "https://runtime.test/current"
        );
        assert_eq!(
            pick(
                None,
                None,
                Some("https://stored.test/last?private=yes#f"),
                true
            ),
            "https://stored.test/last"
        );
        assert_eq!(
            pick(None, None, Some("https://stored.test/last"), false),
            DEFAULT_URL
        );
    }
}
