//! A local tool server that asks which session calls it: its URL carries
//! `{dccSessionId}`, which each session's configuration fills with that
//! session's id. Only a server whose URL asks gets it, and only one on this
//! machine, so a remote server never learns a session's id.

use url::Url;

/// What a server's URL writes where the session's id goes.
pub const SESSION_MARKER: &str = "{dccSessionId}";

/// `url` with the marker filled with `session_id`, for a loopback server;
/// any other URL as it is.
pub fn with_session(url: &str, session_id: &str) -> String {
    let encoded = "%7BdccSessionId%7D";
    if !url.contains(SESSION_MARKER) && !url.contains(encoded) {
        return url.to_string();
    }
    let local = Url::parse(url)
        .ok()
        .is_some_and(|parsed| match parsed.host() {
            Some(url::Host::Domain(host)) => host == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        });
    if !local {
        return url.to_string();
    }
    let id: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    url.replace(SESSION_MARKER, &id).replace(encoded, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_server_that_asks_gets_the_session() {
        assert_eq!(
            with_session(
                "http://127.0.0.1:3100/mcp?bot=b&session={dccSessionId}",
                "s-1"
            ),
            "http://127.0.0.1:3100/mcp?bot=b&session=s-1"
        );
        assert_eq!(
            with_session("http://localhost:9/mcp?s=%7BdccSessionId%7D", "s-1"),
            "http://localhost:9/mcp?s=s-1"
        );
    }

    #[test]
    fn a_remote_server_or_one_that_does_not_ask_is_left_as_it_is() {
        let remote = "https://mcp.example.com/mcp?session={dccSessionId}";
        assert_eq!(with_session(remote, "s-1"), remote);
        let plain = "http://127.0.0.1:3100/mcp?bot=b";
        assert_eq!(with_session(plain, "s-1"), plain);
        assert_eq!(
            with_session("http://127.0.0.1:1/m?s={dccSessionId}", "a&b=c"),
            "http://127.0.0.1:1/m?s=abc"
        );
    }
}
