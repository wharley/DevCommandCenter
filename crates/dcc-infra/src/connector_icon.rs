//! Finds a connector's logo on its own site: the page's `apple-touch-icon` or
//! `icon` link, then the conventional icon paths. The result is a bounded `data:` URI the
//! UI shows in an `<img>`, never inline markup.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use url::Url;

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PAGE_BYTES: usize = 512 * 1024;
const MAX_ICON_BYTES: usize = 256 * 1024;

/// Returns the site's icon for the connector at `server_url`, or `None`.
pub async fn fetch_site_icon(server_url: &str) -> Option<String> {
    let server = Url::parse(server_url).ok()?;
    if !allowed(&server) {
        return None;
    }
    let client = reqwest::Client::builder()
        .connect_timeout(TIMEOUT)
        .timeout(TIMEOUT)
        .no_proxy()
        // Sites commonly redirect `/` and their icon paths; each hop is
        // checked against the same HTTPS rule below.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 || !allowed(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .user_agent(concat!("DevCommandCenter/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?;

    let mut origin = server.clone();
    origin.set_path("/");
    origin.set_query(None);
    origin.set_fragment(None);

    let mut candidates = Vec::new();
    if let Some(page) = download(&client, &origin, MAX_PAGE_BYTES).await {
        let html = String::from_utf8_lossy(&page.1);
        candidates.extend(
            icon_links(&html)
                .into_iter()
                .filter_map(|href| origin.join(&href).ok())
                .filter(allowed),
        );
    }
    // Conventional paths for sites whose pages declare no icon (Next.js
    // `app/icon.png`, for one).
    for path in ["/apple-touch-icon.png", "/icon.png", "/favicon.png", "/favicon.ico"] {
        if let Ok(candidate) = origin.join(path) {
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }

    for candidate in candidates {
        if let Some((content_type, bytes)) = download(&client, &candidate, MAX_ICON_BYTES).await {
            if let Some(media_type) = image_type(content_type.as_deref(), &bytes) {
                return Some(format!(
                    "data:{media_type};base64,{}",
                    STANDARD.encode(&bytes)
                ));
            }
        }
    }
    None
}

fn allowed(url: &Url) -> bool {
    let loopback = matches!(url.host(), Some(url::Host::Domain("localhost")))
        || matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback());
    (url.scheme() == "https" || (url.scheme() == "http" && loopback))
        && url.username().is_empty()
        && url.password().is_none()
}

async fn download(
    client: &reqwest::Client,
    url: &Url,
    limit: usize,
) -> Option<(Option<String>, Vec<u8>)> {
    let mut response = client.get(url.clone()).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        });
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if body.len() + chunk.len() > limit {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    Some((content_type, body))
}

/// `href`s of icon links, the larger `apple-touch-icon` first.
fn icon_links(html: &str) -> Vec<String> {
    let mut touch = Vec::new();
    let mut icons = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(start) = lower[offset..].find("<link") {
        let start = offset + start;
        let Some(end) = lower[start..].find('>') else {
            break;
        };
        let tag = &html[start..start + end];
        offset = start + end;
        let Some(rel) = attribute(tag, "rel") else {
            continue;
        };
        let rel = rel.to_ascii_lowercase();
        let Some(href) = attribute(tag, "href").filter(|href| !href.starts_with("data:")) else {
            continue;
        };
        if rel
            .split_whitespace()
            .any(|token| token == "apple-touch-icon")
        {
            touch.push(href);
        } else if rel.split_whitespace().any(|token| token == "icon") {
            icons.push(href);
        }
    }
    touch.into_iter().chain(icons).collect()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(found) = lower[offset..].find(name) {
        let at = offset + found;
        offset = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value = rest[1..].trim_start();
        let parsed = match value.chars().next()? {
            quote @ ('"' | '\'') => value[1..].split(quote).next()?.to_string(),
            _ => value
                .split(|character: char| character.is_whitespace() || character == '>')
                .next()?
                .trim_end_matches('/')
                .to_string(),
        };
        return Some(parsed.trim().to_string()).filter(|value| !value.is_empty());
    }
    None
}

/// Sniffs the bytes; the declared type alone is not trusted.
fn image_type(declared: Option<&str>, bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(&[0, 0, 1, 0]) {
        return Some("image/x-icon");
    }
    let text = std::str::from_utf8(bytes).ok()?.trim_start();
    let looks_svg =
        text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg"));
    (looks_svg && declared.is_none_or(|kind| kind.contains("svg") || kind.contains("xml")))
        .then_some("image/svg+xml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_touch_icons_before_plain_icons() {
        let html = r#"<head>
            <link rel="stylesheet" href="/app.css">
            <link rel="icon" href="/icon.png" sizes="32x32">
            <LINK REL="apple-touch-icon" HREF='/apple-icon.png'>
            <link rel="shortcut icon" href=/favicon.ico>
            <link rel="icon" href="data:image/png;base64,AAAA">
        </head>"#;
        assert_eq!(
            icon_links(html),
            vec!["/apple-icon.png", "/icon.png", "/favicon.ico"]
        );
    }

    #[test]
    fn identifies_images_by_their_bytes() {
        assert_eq!(
            image_type(Some("text/html"), b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(image_type(None, &[0, 0, 1, 0, 1]), Some("image/x-icon"));
        assert_eq!(
            image_type(Some("image/svg+xml"), b"<svg xmlns=''></svg>"),
            Some("image/svg+xml")
        );
        assert_eq!(image_type(Some("text/html"), b"<svg></svg>"), None);
        assert_eq!(image_type(Some("text/html"), b"<!doctype html>"), None);
    }

    #[test]
    fn only_https_or_loopback_sites_are_fetched() {
        assert!(allowed(&Url::parse("https://app.zedy.com.br/mcp").unwrap()));
        assert!(allowed(&Url::parse("http://localhost:3000/mcp").unwrap()));
        assert!(!allowed(&Url::parse("http://example.com/mcp").unwrap()));
    }

    #[tokio::test]
    #[ignore = "reaches a live site"]
    async fn live_site_icon() {
        let url = std::env::var("DCC_ICON_PROBE_URL").expect("DCC_ICON_PROBE_URL");
        let icon = fetch_site_icon(&url).await.expect("an icon");
        println!("{} {}", &icon[..icon.find(',').unwrap()], icon.len());
    }
}
