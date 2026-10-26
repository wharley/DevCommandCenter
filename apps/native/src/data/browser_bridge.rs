//! The desktop's agent tools on this app's Browser
//! (`/api/v1/browser/bridge/*`): the page operations the agents ask for
//! arrive on one stream and each answer goes back by its id; the Browser
//! tells the desktop when it opens (getting the desktop's lifecycle token),
//! what its page does, whether it shows, and when it closes; and the
//! person arms or disarms the agent's control, captures evidence and reads
//! the Browser's activity. Blocking: call it off the UI thread.

use std::net::TcpStream;
use std::time::Duration;

use dcc_core::domain::browser_evidence::EvidenceResult;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::sse::{Body, SseFrames};

/// A page operation the desktop asks for.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BridgeOpKind {
    /// Runs a script; its result's JSON goes back.
    Eval {
        script: String,
    },
    /// Runs a script; nothing goes back.
    EvalDetached {
        script: String,
    },
    Navigate {
        url: String,
    },
    Reload,
    /// A real click at these fractions of the viewport.
    Click {
        x: f64,
        y: f64,
    },
    /// One allowed key to the focused page.
    Press {
        key: String,
    },
    /// The viewport as PNG, base64 in the answer.
    Snapshot,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeOp {
    pub op_id: u64,
    #[serde(flatten)]
    pub kind: BridgeOpKind,
}

/// What the page did, for the desktop's bookkeeping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageReport {
    Loading(String),
    Loaded(String),
    /// The URL moved without a load (a single-page app).
    Url(String),
    Title(String),
    Visibility(bool),
}

impl PageReport {
    fn json(&self) -> Value {
        match self {
            Self::Loading(url) => json!({ "kind": "loading", "url": url }),
            Self::Loaded(url) => json!({ "kind": "loaded", "url": url }),
            Self::Url(url) => json!({ "kind": "url", "url": url }),
            Self::Title(title) => json!({ "kind": "title", "title": title }),
            Self::Visibility(visible) => json!({ "kind": "visibility", "visible": visible }),
        }
    }
}

/// `BrowserControlStatus`: whether the agent may act, and for how long.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ControlStatus {
    pub armed: bool,
    pub remaining_ms: u64,
}

/// "Capturar evidência" started: the capture's handle and the page as it
/// was then.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EvidenceStart {
    pub capture_id: String,
    pub remaining_ms: u64,
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub selection_only: bool,
    pub truncated: bool,
}

/// One line of "Atividade do navegador" (`BrowserAuditViewRecord`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditRecord {
    /// `ui` or `mcp`.
    pub origin: String,
    pub provider_id: Option<String>,
    pub tool: String,
    pub grant_state: String,
    pub outcome: String,
    pub timestamp_ms: i64,
}

/// The operations, one at a time; `None` once the stream ends.
pub struct BridgeOps {
    body: Body,
    frames: SseFrames,
}

impl BridgeOps {
    /// Parses one frame of the stream (`browser-op`; others are skipped).
    pub fn op(name: &str, data: &str) -> Option<BridgeOp> {
        (name == "browser-op")
            .then(|| serde_json::from_str(data).ok())
            .flatten()
    }
}

impl Iterator for BridgeOps {
    type Item = BridgeOp;

    fn next(&mut self) -> Option<BridgeOp> {
        use std::io::Read;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            while let Some((name, data)) = self.frames.next_frame() {
                if let Some(op) = Self::op(&name, &data) {
                    return Some(op);
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
    /// The operations for this app's Browser, until the desktop closes;
    /// with the socket, to end the stream when the Browser closes.
    pub fn browser_ops(&self) -> Result<(BridgeOps, Option<TcpStream>), ApiError> {
        let (reader, chunked) =
            self.open_stream("/api/v1/browser/bridge/stream", Duration::from_secs(45))?;
        let socket = reader.get_ref().try_clone().ok();
        Ok((
            BridgeOps {
                body: Body::new(reader, chunked),
                frames: SseFrames::default(),
            },
            socket,
        ))
    }

    /// The Browser opened on `url`: the desktop's lifecycle token for it.
    pub fn browser_surface_open(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        url: &str,
    ) -> Result<u64, ApiError> {
        let value = self.post(
            "/api/v1/browser/bridge/surface",
            &json!({ "workspaceId": workspace_id, "sessionId": session_id, "url": url }),
        )?;
        value
            .get("lifecycleToken")
            .and_then(Value::as_u64)
            .ok_or_else(|| ApiError::Failed("no lifecycle token".into()))
    }

    pub fn browser_report(&self, token: u64, report: &PageReport) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/browser/bridge/surface/{token}/report"),
            &report.json(),
        )
        .map(|_| ())
    }

    pub fn browser_close(&self, token: u64) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/browser/bridge/surface/{token}/close"),
            &json!({}),
        )
        .map(|_| ())
    }

    /// An operation's answer: the script's JSON, or why it failed.
    pub fn browser_reply(
        &self,
        op_id: u64,
        result: &Result<String, String>,
    ) -> Result<(), ApiError> {
        let body = match result {
            Ok(raw) => json!({ "ok": true, "raw": raw }),
            Err(error) => json!({ "ok": false, "error": error }),
        };
        self.post(&format!("/api/v1/browser/bridge/ops/{op_id}/reply"), &body)
            .map(|_| ())
    }

    /// Arms (`Some(true)`), disarms (`Some(false)`) or reads the control.
    pub fn browser_control(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        token: u64,
        armed: Option<bool>,
    ) -> Result<ControlStatus, ApiError> {
        Self::decoded(self.post(
            "/api/v1/browser/bridge/control",
            &json!({
                "workspaceId": workspace_id,
                "sessionId": session_id,
                "lifecycleToken": token,
                "armed": armed,
            }),
        )?)
    }

    /// Reads the page and starts a capture anchored on it (needs the grant).
    pub fn browser_evidence_start(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        token: u64,
    ) -> Result<EvidenceStart, ApiError> {
        Self::decoded(self.post(
            "/api/v1/browser/bridge/evidence/start",
            &json!({
                "workspaceId": workspace_id,
                "sessionId": session_id,
                "lifecycleToken": token,
            }),
        )?)
    }

    /// The capture's events; the handle is spent either way.
    pub fn browser_evidence_read(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        capture_id: &str,
    ) -> Result<EvidenceResult, ApiError> {
        Self::decoded(self.post(
            "/api/v1/browser/bridge/evidence/read",
            &json!({
                "workspaceId": workspace_id,
                "sessionId": session_id,
                "captureId": capture_id,
            }),
        )?)
    }

    /// `readBrowserAudit`, newest first (`newestFirstBrowserAuditRecords`).
    pub fn browser_audit(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        token: u64,
        limit: usize,
    ) -> Result<Vec<AuditRecord>, ApiError> {
        let value = self.post(
            "/api/v1/browser/bridge/audit",
            &json!({
                "workspaceId": workspace_id,
                "sessionId": session_id,
                "lifecycleToken": token,
                "limit": limit,
            }),
        )?;
        let mut records: Vec<AuditRecord> =
            Self::decoded(value.get("records").cloned().unwrap_or(Value::Null))?;
        records.sort_by_key(|record| std::cmp::Reverse(record.timestamp_ms));
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_streams_operations_read_like_the_desktop_sends_them() {
        assert_eq!(
            BridgeOps::op("browser-op", r#"{"opId":7,"kind":"eval","script":"1"}"#),
            Some(BridgeOp {
                op_id: 7,
                kind: BridgeOpKind::Eval { script: "1".into() },
            })
        );
        assert_eq!(
            BridgeOps::op("browser-op", r#"{"opId":8,"kind":"reload"}"#),
            Some(BridgeOp {
                op_id: 8,
                kind: BridgeOpKind::Reload,
            })
        );
        assert_eq!(
            BridgeOps::op(
                "browser-op",
                r#"{"opId":9,"kind":"click","x":0.25,"y":0.5}"#
            ),
            Some(BridgeOp {
                op_id: 9,
                kind: BridgeOpKind::Click { x: 0.25, y: 0.5 },
            })
        );
        assert_eq!(
            BridgeOps::op("browser-op", r#"{"opId":10,"kind":"snapshot"}"#),
            Some(BridgeOp {
                op_id: 10,
                kind: BridgeOpKind::Snapshot,
            })
        );
        assert_eq!(BridgeOps::op("browser-lagged", "{}"), None);
        assert_eq!(
            PageReport::Visibility(false).json(),
            json!({ "kind": "visibility", "visible": false })
        );
    }
}
