//! The app's HTTP client for the images the markdown draws
//! (`markdown::image`): GPUI fetches a remote image through it and has none
//! by default. Only `https`, at most 5 redirects (all `https`), 15 seconds
//! and 10 MB; each fetch on a thread of its own with the system's TLS
//! (`reqwest`, the version the backend already uses).

use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use futures::future::BoxFuture;
use gpui::http_client::http::HeaderValue;
use gpui::http_client::{AsyncBody, HttpClient, Request, Response, Url};

const MAX_BYTES: u64 = 10 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
const AGENT: &str = "DevCommandCenter";

pub struct ImageHttp {
    agent: HeaderValue,
}

impl ImageHttp {
    pub fn new() -> Self {
        Self {
            agent: HeaderValue::from_static(AGENT),
        }
    }
}

fn client() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            let redirects = reqwest::redirect::Policy::custom(|attempt| {
                if attempt.url().scheme() != "https" || attempt.previous().len() > 5 {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            });
            reqwest::blocking::Client::builder()
                .user_agent(AGENT)
                .timeout(TIMEOUT)
                .redirect(redirects)
                .build()
                .ok()
        })
        .as_ref()
}

/// Whether a request may go out.
fn allowed(uri: &str) -> bool {
    uri.starts_with("https://")
}

fn fetch(uri: &str) -> anyhow::Result<Response<AsyncBody>> {
    anyhow::ensure!(allowed(uri), "only https images load");
    let client = client().ok_or_else(|| anyhow::anyhow!("no http client"))?;
    let response = client.get(uri).send()?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= MAX_BYTES, "image larger than 10 MB");
    Ok(Response::builder()
        .status(status)
        .body(AsyncBody::from(bytes))?)
}

impl HttpClient for ImageHttp {
    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let uri = request.uri().to_string();
        let (answer, answered) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = answer.send(fetch(&uri));
        });
        Box::pin(async move {
            answered
                .await
                .map_err(|_| anyhow::anyhow!("image fetch dropped"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_goes_out() {
        assert!(allowed("https://vercel.com/static/status/ready.svg"));
        assert!(!allowed("http://example.com/a.png"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(fetch("http://example.com/a.png").is_err());
    }
}
