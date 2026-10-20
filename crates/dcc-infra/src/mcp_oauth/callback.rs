//! Minimal loopback HTTP endpoint for the OAuth redirect. It accepts one
//! `GET /oauth/mcp/callback?…` per connection, hands the query to the caller,
//! and answers with a static page. Nothing else is served.

use std::{
    io,
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use url::Url;

use super::{flow::CallbackParams, DCC_OAUTH_CALLBACK_PATH};

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct BoundCallbackListener {
    pub listener: TcpListener,
    /// The same port on `::1`, so a `localhost` redirect lands whichever
    /// loopback address the browser tries. Absent when IPv6 is off.
    pub listener_v6: Option<TcpListener>,
    pub port: u16,
    /// False when the fixed port was taken and an ephemeral one was used.
    pub fixed_port: bool,
}

/// Binds the fixed callback port on 127.0.0.1 (and on ::1 when it can),
/// falling back to an ephemeral port when another process holds it.
pub async fn bind_callback_listener(preferred_port: u16) -> io::Result<BoundCallbackListener> {
    let (listener, fixed_port) =
        match TcpListener::bind((Ipv4Addr::LOCALHOST, preferred_port)).await {
            Ok(listener) => (listener, true),
            Err(_) => (TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?, false),
        };
    let port = listener.local_addr()?.port();
    let listener_v6 = TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await.ok();
    Ok(BoundCallbackListener {
        listener,
        listener_v6,
        port,
        fixed_port,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallbackPage {
    Done,
    Failed,
    NotFound,
}

/// Reads one request, lets `handle` decide the outcome, and writes the page.
pub async fn serve_callback_connection(
    mut stream: TcpStream,
    handle: impl FnOnce(CallbackParams) -> CallbackPage,
) {
    let mut buffer = Vec::with_capacity(1024);
    let read = timeout(READ_TIMEOUT, async {
        let mut chunk = [0_u8; 1024];
        loop {
            let count = stream.read(&mut chunk).await?;
            if count == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..count]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n")
                || buffer.len() >= MAX_REQUEST_BYTES
            {
                break;
            }
        }
        io::Result::Ok(())
    })
    .await;
    let page = match (read, parse_callback_request(&buffer)) {
        (Ok(Ok(())), Some(params)) => handle(params),
        _ => CallbackPage::NotFound,
    };
    let _ = stream.write_all(callback_page(&page).as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Returns the query of a `GET` to the callback path, or `None`.
pub fn parse_callback_request(request: &[u8]) -> Option<CallbackParams> {
    let text = std::str::from_utf8(request).ok()?;
    let request_line = text.lines().next()?;
    let mut parts = request_line.split(' ');
    let (method, target) = (parts.next()?, parts.next()?);
    if method != "GET" || !parts.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let url = Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != DCC_OAUTH_CALLBACK_PATH {
        return None;
    }
    let mut params = CallbackParams::default();
    for (name, value) in url.query_pairs() {
        let value = Some(value.into_owned());
        match name.as_ref() {
            "code" => params.code = value,
            "state" => params.state = value,
            "iss" => params.iss = value,
            "error" => params.error = value,
            "error_description" => params.error_description = value,
            _ => {}
        }
    }
    Some(params)
}

pub fn callback_page(page: &CallbackPage) -> String {
    let (status, title, message) = match page {
        CallbackPage::Done => (
            "200 OK",
            "Pronto",
            "Pode voltar ao Dev Command Center. · You can return to Dev Command Center.",
        ),
        CallbackPage::Failed => (
            "200 OK",
            "Não foi possível entrar",
            "Volte ao Dev Command Center para ver o motivo. · Return to Dev Command Center to see why.",
        ),
        CallbackPage::NotFound => ("404 Not Found", "Não encontrado", "Nothing here."),
    };
    let body = format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{title}</title>\
<style>body{{font:16px -apple-system,system-ui,sans-serif;background:#0b0b0c;color:#ededed;display:grid;place-items:center;height:100vh;margin:0}}\
main{{text-align:center;max-width:28rem;padding:2rem}}h1{{font-size:1.25rem;margin:0 0 .5rem}}p{{color:#a1a1aa;margin:0}}</style>\
</head><body><main><h1>{title}</h1><p>{message}</p></main></body></html>"
    );
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_apps_register_the_localhost_redirect() {
        // The desktop app shows and the Slack manifest carries this exact
        // string (connectors/user-app.ts).
        assert_eq!(
            super::super::user_app_redirect_uri(super::super::DCC_OAUTH_CALLBACK_PORT),
            "http://localhost:46327/oauth/mcp/callback"
        );
    }

    #[tokio::test]
    async fn answers_on_both_loopback_addresses() {
        let bound = bind_callback_listener(0).await.expect("listener");
        if let Some(listener_v6) = &bound.listener_v6 {
            assert_eq!(listener_v6.local_addr().unwrap().port(), bound.port);
        }
    }

    #[test]
    fn parses_the_callback_query() {
        let params = parse_callback_request(
            b"GET /oauth/mcp/callback?code=abc&state=xyz&iss=https%3A%2F%2Fauth.example.com HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        )
        .expect("callback");
        assert_eq!(params.code.as_deref(), Some("abc"));
        assert_eq!(params.state.as_deref(), Some("xyz"));
        assert_eq!(params.iss.as_deref(), Some("https://auth.example.com"));
    }

    #[test]
    fn ignores_other_paths_and_methods() {
        assert!(parse_callback_request(b"GET /favicon.ico HTTP/1.1\r\n\r\n").is_none());
        assert!(parse_callback_request(b"POST /oauth/mcp/callback HTTP/1.1\r\n\r\n").is_none());
        assert!(parse_callback_request(b"garbage").is_none());
    }

    #[test]
    fn pages_are_locked_down() {
        let page = callback_page(&CallbackPage::Done);
        assert!(page.contains("Content-Security-Policy: default-src 'none'"));
        assert!(page.contains("Cache-Control: no-store"));
        assert!(page.contains("Referrer-Policy: no-referrer"));
    }

    #[tokio::test]
    async fn falls_back_to_an_ephemeral_port_when_taken() {
        let first = bind_callback_listener(0).await.expect("bind");
        let second = bind_callback_listener(first.port).await.expect("fallback");
        assert!(!second.fixed_port);
        assert_ne!(second.port, first.port);
    }
}
