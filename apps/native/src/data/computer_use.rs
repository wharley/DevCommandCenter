//! Computer use through the desktop (`/api/v1/computer/bridge/*`): the
//! agents' control requests (a stream that this app owns while it
//! listens), the Mac's state for one, a permission's settings, and the
//! answer. The desktop does the controlling; its permissions are its own.
//! Blocking: call it off the UI thread.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi, encode};
use crate::data::sse::{Body, SseFrames};

/// `ComputerUsePendingRequest`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ControlRequest {
    pub request_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub provider_id: String,
    pub reason: String,
    pub remaining_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Permission {
    pub granted: bool,
    pub can_request: bool,
}

/// An app with a window the agent may be allowed to control.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Target {
    pub bundle_id: String,
    pub name: String,
}

/// `ComputerUseStatus`, what the card needs of it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ComputerStatus {
    pub supported: bool,
    pub runtime_attached: bool,
    pub accessibility: Permission,
    pub screen_recording: Permission,
    pub targets: Vec<Target>,
}

impl ComputerStatus {
    /// The apps, each once (`targets` lists windows).
    pub fn apps(&self) -> Vec<Target> {
        let mut seen = std::collections::HashSet::new();
        self.targets
            .iter()
            .filter(|target| seen.insert(target.bundle_id.clone()))
            .cloned()
            .collect()
    }

    pub fn permissions_ready(&self) -> bool {
        self.accessibility.granted && self.screen_recording.granted
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessKind {
    Accessibility,
    ScreenRecording,
}

impl AccessKind {
    fn name(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::ScreenRecording => "screenRecording",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputerEvent {
    /// The control requests waiting, soonest to expire first.
    Requests(Vec<ControlRequest>),
}

pub struct ComputerStream {
    body: Body,
    frames: SseFrames,
}

impl ComputerStream {
    pub fn event(name: &str, data: &str) -> Option<ComputerEvent> {
        match name {
            "control-requests" => serde_json::from_str(data).ok().map(ComputerEvent::Requests),
            _ => None,
        }
    }
}

impl Iterator for ComputerStream {
    type Item = ComputerEvent;

    fn next(&mut self) -> Option<ComputerEvent> {
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
    pub fn computer_events(&self) -> Result<ComputerStream, ApiError> {
        let (reader, chunked) =
            self.open_stream("/api/v1/computer/bridge/stream", Duration::from_secs(45))?;
        Ok(ComputerStream {
            body: Body::new(reader, chunked),
            frames: SseFrames::default(),
        })
    }

    /// The Mac's state for a conversation, with its apps.
    pub fn computer_status(&self, session_id: &str) -> Result<ComputerStatus, ApiError> {
        Self::decoded(self.post(
            "/api/v1/computer/bridge/status",
            &json!({ "sessionId": session_id, "includeTargets": true }),
        )?)
    }

    /// Opens the permission's system settings.
    pub fn computer_access(&self, kind: AccessKind) -> Result<ComputerStatus, ApiError> {
        Self::decoded(self.post(
            "/api/v1/computer/bridge/access",
            &json!({ "kind": kind.name() }),
        )?)
    }

    pub fn computer_respond(
        &self,
        request_id: &str,
        allowed: bool,
        bundle_ids: &[String],
    ) -> Result<(), ApiError> {
        self.post(
            &format!(
                "/api/v1/computer/bridge/requests/{}/respond",
                encode(request_id)
            ),
            &json!({ "allowed": allowed, "allowedBundleIds": bundle_ids }),
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_requests_and_the_status_read_like_the_desktop_sends_them() {
        let Some(ComputerEvent::Requests(requests)) = ComputerStream::event(
            "control-requests",
            r#"[{"requestId":"ab","sessionId":"s","workspaceId":"w","providerId":"codex","reason":"abrir o Finder","remainingMs":90000}]"#,
        ) else {
            panic!("the list");
        };
        assert_eq!(requests[0].remaining_ms, 90_000);
        let status: ComputerStatus = serde_json::from_str(
            r#"{"platform":"macos","supported":true,"runtimeAttached":true,"accessibility":{"granted":true,"canRequest":false},"screenRecording":{"granted":false,"canRequest":true},"grant":{"armed":false},"targets":[{"bundleId":"com.apple.finder","name":"Finder","pid":1},{"bundleId":"com.apple.finder","name":"Finder","pid":1},{"bundleId":"com.apple.Safari","name":"Safari"}]}"#,
        )
        .unwrap();
        assert!(!status.permissions_ready());
        let apps: Vec<String> = status.apps().into_iter().map(|app| app.name).collect();
        assert_eq!(apps, ["Finder", "Safari"]);
    }
}
