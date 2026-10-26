//! The agents' requests to open a page in the Browser
//! (`/api/v1/browser/bridge/opens/*`). While this app listens on the stream
//! it answers them instead of the desktop: the stream brings the requests
//! waiting for the person (again whenever they change) and the ones a
//! phone allowed; "Abrir e permitir" and "Negar" answer one (first answer
//! wins), and the open is confirmed with the Browser's lifecycle token.
//! Blocking: call it off the UI thread.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi, encode};
use crate::data::sse::{Body, SseFrames};

/// `BrowserOpenRequest`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenRequest {
    pub request_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub provider_id: String,
    pub url: String,
    pub reason: String,
    pub expires_at_ms: u64,
    /// The task needs no prompt; it opens at once while it is on screen.
    pub preapproved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpensEvent {
    /// The requests waiting, soonest to expire first.
    Waiting(Vec<OpenRequest>),
    /// Allowed elsewhere (a phone, the desktop's panel): open it.
    Approved(OpenRequest),
}

/// The stream's events, one at a time; `None` once it ends.
pub struct OpensStream {
    body: Body,
    frames: SseFrames,
}

impl OpensStream {
    /// Parses one frame (`open-requests`, `open-approved`; others skipped).
    pub fn event(name: &str, data: &str) -> Option<OpensEvent> {
        match name {
            "open-requests" => serde_json::from_str(data).ok().map(OpensEvent::Waiting),
            "open-approved" => serde_json::from_str(data).ok().map(OpensEvent::Approved),
            _ => None,
        }
    }
}

impl Iterator for OpensStream {
    type Item = OpensEvent;

    fn next(&mut self) -> Option<OpensEvent> {
        use std::io::Read;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            while let Some((name, data)) = self.frames.next_frame() {
                if let Some(event) = Self::event(&name, &data) {
                    return Some(event);
                }
            }
            match self.body.read(&mut buffer) {
                Ok(0) | Err(_) => return None,
                Ok(read) => self.frames.feed(&buffer[..read]),
            }
        }
    }
}

impl DesktopApi {
    pub fn browser_opens(&self) -> Result<OpensStream, ApiError> {
        let (reader, chunked) = self.open_stream(
            "/api/v1/browser/bridge/opens/stream",
            Duration::from_secs(45),
        )?;
        Ok(OpensStream {
            body: Body::new(reader, chunked),
            frames: SseFrames::default(),
        })
    }

    fn answer_open(
        &self,
        request: &OpenRequest,
        action: &str,
        token: Option<u64>,
    ) -> Result<serde_json::Value, ApiError> {
        self.post(
            &format!(
                "/api/v1/browser/bridge/opens/{}/{action}",
                encode(&request.request_id)
            ),
            &json!({
                "workspaceId": request.workspace_id,
                "sessionId": request.session_id,
                "lifecycleToken": token,
            }),
        )
    }

    /// "Abrir e permitir": the request is this app's to open now.
    pub fn browser_open_allow(&self, request: &OpenRequest) -> Result<(), ApiError> {
        self.answer_open(request, "allow", None).map(|_| ())
    }

    pub fn browser_open_deny(&self, request: &OpenRequest) -> Result<(), ApiError> {
        self.answer_open(request, "deny", None).map(|_| ())
    }

    /// The Browser opened it on this lifecycle: the approval is complete.
    pub fn browser_open_ack(&self, request: &OpenRequest, token: u64) -> Result<(), ApiError> {
        self.answer_open(request, "opened", Some(token)).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_streams_events_read_like_the_desktop_sends_them() {
        let waiting = OpensStream::event(
            "open-requests",
            r#"[{"requestId":"r-1","workspaceId":"w-1","sessionId":"s-1","providerId":"claude","url":"https://ex.com/","reason":"ver","expiresAtMs":9,"preapproved":true}]"#,
        );
        let Some(OpensEvent::Waiting(requests)) = waiting else {
            panic!("the list");
        };
        assert_eq!(requests[0].url, "https://ex.com/");
        assert!(requests[0].preapproved);
        assert!(matches!(
            OpensStream::event("open-approved", r#"{"requestId":"r-2"}"#),
            Some(OpensEvent::Approved(request)) if request.request_id == "r-2"
        ));
        assert_eq!(OpensStream::event("keep-alive", ""), None);
    }
}
