//! The address bar's omnibox (`resolveHumanBrowserAddress`): loopback gets
//! HTTP, domains get HTTPS, words become a search; anything with its own
//! scheme or a file path is left for the URL policy to refuse.

use std::sync::LazyLock;

use regex::Regex;

const SEARCH_URL: &str = "https://www.google.com/search?q=";

static EXPLICIT_SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z\d+.-]*:").expect("scheme pattern"));
static LOOPBACK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:localhost|127(?:\.\d{1,3}){3}|\[::1\])(?::\d+)?(?:[/?#]|$)")
        .expect("loopback pattern")
});
static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?(?:[/?#]|$)").expect("ipv4 pattern")
});
static DOMAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:[a-z\d](?:[a-z\d-]{0,61}[a-z\d])?\.)+[a-z]{2,63}(?::\d+)?(?:[/?#]|$)")
        .expect("domain pattern")
});
static FILE_LIKE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\.{1,2}[\\/]|[~/\\])").expect("file pattern"));

/// `encodeURIComponent`
fn encode_component(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

pub fn resolve(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() {
        return String::new();
    }
    if LOOPBACK.is_match(value) {
        return format!("http://{value}");
    }
    if let Some(rest) = value.strip_prefix("//") {
        return format!("https://{rest}");
    }
    if EXPLICIT_SCHEME.is_match(value) {
        return value.to_string();
    }
    let spaced = value.chars().any(|ch| ch.is_whitespace() || ch == '\\');
    if !spaced && (IPV4.is_match(value) || DOMAIN.is_match(value)) {
        return format!("https://{value}");
    }
    // Filesystem-looking input never goes to a search provider.
    if FILE_LIKE.is_match(value) {
        return value.to_string();
    }
    format!("{SEARCH_URL}{}", encode_component(value))
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn explicit_web_urls_stay() {
        assert_eq!(
            resolve("  https://example.com/docs?q=1  "),
            "https://example.com/docs?q=1"
        );
        assert_eq!(
            resolve("http://localhost:5173/app"),
            "http://localhost:5173/app"
        );
    }

    #[test]
    fn local_addresses_get_http() {
        assert_eq!(resolve("localhost:5173"), "http://localhost:5173");
        assert_eq!(resolve("127.0.0.1:3000/path"), "http://127.0.0.1:3000/path");
        assert_eq!(resolve("[::1]:8080"), "http://[::1]:8080");
    }

    #[test]
    fn domains_and_protocol_relative_urls_get_https() {
        assert_eq!(resolve("google.com"), "https://google.com");
        assert_eq!(
            resolve("docs.example.com/guide"),
            "https://docs.example.com/guide"
        );
        assert_eq!(resolve("//example.com/path"), "https://example.com/path");
    }

    #[test]
    fn words_become_encoded_searches() {
        assert_eq!(resolve("google"), "https://www.google.com/search?q=google");
        assert_eq!(
            resolve("como testar React & Vite"),
            "https://www.google.com/search?q=como%20testar%20React%20%26%20Vite"
        );
    }

    #[test]
    fn other_schemes_and_paths_are_left_alone() {
        assert_eq!(resolve("javascript:alert(1)"), "javascript:alert(1)");
        assert_eq!(resolve("ftp://example.com/file"), "ftp://example.com/file");
        assert_eq!(resolve("./private/file.txt"), "./private/file.txt");
        assert_eq!(
            resolve("/Users/person/private.txt"),
            "/Users/person/private.txt"
        );
        assert_eq!(resolve(""), "");
    }
}
