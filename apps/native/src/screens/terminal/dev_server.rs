//! A local server's address in what a shell prints (`dev-server-detection.ts`):
//! loopback `http(s)` URLs only, the bind-all ones turned into loopback, a
//! URL split across two chunks still found through a 2048-character tail.

use std::sync::LazyLock;

use regex::Regex;

/// `DEV_SERVER_DETECTION_TAIL_CHARS`
pub const TAIL_CHARS: usize = 2048;

static ANSI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))").expect("ansi pattern")
});
/// `LOCAL_URL_CANDIDATE` without its lookahead, which is checked by hand.
static CANDIDATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)https?://(?:localhost|127(?:\.\d{1,3}){3}|0\.0\.0\.0|\[::1?\])(?::\d{1,5})?(?:[/?#][^\s<>"'`\x1b]*)?(?:[),.;\]}])?"#,
    )
    .expect("local url pattern")
});

pub fn strip_ansi(text: &str) -> String {
    ANSI.replace_all(text, "").into_owned()
}

/// What may follow a URL: the end, a space or a delimiter.
fn bounded(text: &str, end: usize) -> bool {
    text[end..]
        .chars()
        .next()
        .is_none_or(|next| next.is_whitespace() || "<>\"'`\x1b".contains(next))
}

fn ipv4_loopback(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts[0] == "127"
        && parts.iter().all(|part| {
            (1..=3).contains(&part.len())
                && part.chars().all(|ch| ch.is_ascii_digit())
                && part.parse::<u16>().is_ok_and(|value| value <= 255)
        })
}

/// `normalizeLocalUrl`
fn normalize(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim_end_matches([')', ',', '.', ';', ']', '}']);
    let mut parsed = url::Url::parse(trimmed).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    match host.as_str() {
        "localhost" | "[::1]" => {}
        "0.0.0.0" => parsed.set_host(Some("127.0.0.1")).ok()?,
        "[::]" => parsed.set_host(Some("[::1]")).ok()?,
        host if ipv4_loopback(host) => {}
        _ => return None,
    }
    Some(parsed.to_string())
}

fn matches(text: &str) -> impl Iterator<Item = regex::Match<'_>> {
    CANDIDATE
        .find_iter(text)
        .filter(move |found| bounded(text, found.end()))
}

/// `detectLocalDevServerUrls`
pub fn detect(output: &str) -> Vec<String> {
    let plain = strip_ansi(output);
    let mut urls: Vec<String> = Vec::new();
    for found in matches(&plain) {
        if let Some(url) = normalize(found.as_str())
            && !urls.contains(&url)
        {
            urls.push(url);
        }
    }
    urls
}

/// `scanDevServerOutput`: the new addresses and the tail to keep.
pub fn scan(previous_tail: &str, output: &str) -> (Vec<String>, String) {
    let combined = format!("{previous_tail}{}", strip_ansi(output));
    let mut urls: Vec<String> = Vec::new();
    for found in matches(&combined) {
        // Wholly inside the old tail: already seen.
        if found.end() <= previous_tail.len() {
            continue;
        }
        if let Some(url) = normalize(found.as_str())
            && !urls.contains(&url)
        {
            urls.push(url);
        }
    }
    let count = combined.chars().count();
    let tail = combined
        .chars()
        .skip(count.saturating_sub(TAIL_CHARS))
        .collect();
    (urls, tail)
}

/// A server seen in a shell, newest first (`detectedAt` orders them across
/// the scope's tabs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub url: String,
    pub at: u64,
}

/// `max(Date.now(), latest + 1)`: later detections always sort later.
fn stamp() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LATEST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let mut previous = LATEST.load(Ordering::Relaxed);
    loop {
        let next = now.max(previous + 1);
        match LATEST.compare_exchange(previous, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(actual) => previous = actual,
        }
    }
}

/// One shell's detections (`detectedDevServers`, the last five).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DevServers {
    pub found: Vec<Detected>,
    tail: String,
}

impl DevServers {
    /// Returns whether a server was found.
    pub fn feed(&mut self, output: &str) -> bool {
        let (urls, tail) = scan(&self.tail, output);
        self.tail = tail;
        for url in &urls {
            self.found.retain(|server| &server.url != url);
            self.found.insert(
                0,
                Detected {
                    url: url.clone(),
                    at: stamp(),
                },
            );
            self.found.truncate(5);
        }
        !urls.is_empty()
    }
}

/// `formatDevServerAddress`: `host:port`.
pub fn address(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            let host = parsed.host_str()?.to_string();
            Some(match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host,
            })
        })
        .unwrap_or_else(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_localhost_output() {
        assert_eq!(
            detect("\x1b[32m➜\x1b[0m Local: http://localhost:5173/\r\n"),
            ["http://localhost:5173/"]
        );
    }

    #[test]
    fn ipv4_and_ipv6_loopback() {
        assert_eq!(
            detect("API http://127.0.0.1:8000/docs UI https://[::1]:3000/app"),
            ["http://127.0.0.1:8000/docs", "https://[::1]:3000/app"]
        );
    }

    #[test]
    fn bind_all_addresses_become_loopback() {
        assert_eq!(
            detect("Listening at http://0.0.0.0:4321 and http://[::]:8080"),
            ["http://127.0.0.1:4321/", "http://[::1]:8080/"]
        );
    }

    #[test]
    fn remote_lan_malformed_and_other_schemes_are_ignored() {
        assert!(
            detect(
                "https://example.com http://192.168.1.10:5173 ftp://localhost:21 http://127.999.0.1:9 http://localhost:3000.example.com"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_url_split_across_chunks_is_found_once() {
        let (urls, tail) = scan("", "ready at http://local");
        assert!(urls.is_empty());
        let (urls, tail) = scan(&tail, "host:5173/\r\n");
        assert_eq!(urls, ["http://localhost:5173/"]);
        let (urls, _) = scan(&tail, "compiled successfully\r\n");
        assert!(urls.is_empty());
    }

    #[test]
    fn a_shell_keeps_its_five_latest_servers() {
        let mut servers = DevServers::default();
        assert!(servers.feed("at http://localhost:3000/\r\n"));
        assert!(!servers.feed("compiling\r\n"));
        for port in 1..=5 {
            servers.feed(&format!("http://localhost:400{port}/\r\n"));
        }
        servers.feed("again http://localhost:4002/\r\n");
        let urls: Vec<_> = servers
            .found
            .iter()
            .map(|server| server.url.as_str())
            .collect();
        assert_eq!(
            urls,
            [
                "http://localhost:4002/",
                "http://localhost:4005/",
                "http://localhost:4004/",
                "http://localhost:4003/",
                "http://localhost:4001/",
            ]
        );
        assert!(servers.found[0].at > servers.found[1].at);
    }

    #[test]
    fn the_address_is_compact() {
        assert_eq!(address("http://localhost:5173/app"), "localhost:5173");
    }
}
