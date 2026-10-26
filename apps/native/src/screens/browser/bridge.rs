//! This Browser as the desktop's agent surface
//! (`http_api::browser_bridge`). When its page exists it listens for the
//! agents' page operations — the stream connects first, so the desktop
//! never sees it gone between two Browsers — then tells the desktop it
//! opened and keeps its lifecycle token. Every report and every answer
//! goes out in order on one thread; the operations run on the page here.
//! Closing the Browser tells the desktop and ends the stream.

use std::net::TcpStream;
use std::sync::{Arc, Mutex, mpsc};

use dcc_core::domain::browser::validate_browser_url;
use futures::StreamExt;
use gpui::{Context, Task};

use super::surface::{BrowserEvent, BrowserSurface};
use crate::data::browser_bridge::{BridgeOp, BridgeOpKind, PageReport};
use crate::data::desktop_api::DesktopApi;

/// What goes to the desktop, in the order it happened.
pub(super) enum Outgoing {
    Report(PageReport),
    Reply(u64, Result<String, String>),
    Close,
}

pub struct SurfaceBridge {
    outbox: mpsc::Sender<Outgoing>,
    /// The desktop's lifecycle token, once it answered the open.
    pub token: Option<u64>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    _ops: Option<Task<()>>,
    _token: Option<Task<()>>,
}

impl Drop for SurfaceBridge {
    fn drop(&mut self) {
        let _ = self.outbox.send(Outgoing::Close);
        if let Some(socket) = self.socket.lock().ok().and_then(|mut socket| socket.take()) {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl SurfaceBridge {
    /// A bridge whose outgoing messages a test reads.
    #[cfg(test)]
    pub(super) fn for_test(token: u64) -> (Self, mpsc::Receiver<Outgoing>) {
        let (outbox, inbox) = mpsc::channel();
        (
            Self {
                outbox,
                token: Some(token),
                socket: Arc::default(),
                _ops: None,
                _token: None,
            },
            inbox,
        )
    }
}

impl BrowserSurface {
    /// The page exists: the desktop's agents can reach it.
    pub(super) fn attach_bridge(&mut self, url: String, cx: &mut Context<Self>) {
        let Some(api) = self.bridge_api.clone() else {
            return;
        };
        let (outbox, inbox) = mpsc::channel::<Outgoing>();
        let (ops_sender, mut ops) = futures::channel::mpsc::unbounded::<BridgeOp>();
        let (token_sender, token) = futures::channel::oneshot::channel::<Option<u64>>();
        let socket: Arc<Mutex<Option<TcpStream>>> = Arc::default();
        let held = socket.clone();
        let request = self.request.clone();
        std::thread::spawn(move || {
            let stream = api.browser_ops();
            if let (Ok((_, Some(open))), Ok(mut held)) = (&stream, held.lock()) {
                *held = open.try_clone().ok();
            }
            // The reports and answers, once the desktop knows this Browser.
            let reporter = api.clone();
            std::thread::spawn(move || {
                let opened = reporter
                    .browser_surface_open(
                        &request.workspace_id,
                        request.session_id.as_deref(),
                        &url,
                    )
                    .ok();
                let _ = token_sender.send(opened);
                let Some(token) = opened else {
                    return;
                };
                send_out(&reporter, token, inbox);
            });
            if let Ok((stream, _)) = stream {
                for op in stream {
                    if ops_sender.unbounded_send(op).is_err() {
                        break;
                    }
                }
            }
        });
        let ops_task = cx.spawn(async move |this, cx| {
            while let Some(op) = ops.next().await {
                if this
                    .update(cx, |surface, cx| surface.run_op(op, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let token_task = cx.spawn(async move |this, cx| {
            let token = token.await.ok().flatten();
            let _ = this.update(cx, |surface, cx| {
                if let Some(bridge) = surface.bridge.as_mut() {
                    bridge.token = token;
                }
                if let Some(token) = token {
                    cx.emit(BrowserEvent::Ready(token));
                }
                cx.notify();
            });
        });
        self.bridge = Some(SurfaceBridge {
            outbox,
            token: None,
            socket,
            _ops: Some(ops_task),
            _token: Some(token_task),
        });
    }

    /// What the page did, for the desktop (dropped without a bridge).
    pub(super) fn report(&self, report: PageReport) {
        if let Some(bridge) = &self.bridge {
            let _ = bridge.outbox.send(Outgoing::Report(report));
        }
    }

    fn answer(&self, op_id: u64, result: Result<String, String>) {
        if let Some(bridge) = &self.bridge {
            let _ = bridge.outbox.send(Outgoing::Reply(op_id, result));
        }
    }

    /// An agent's page operation, on this Browser's page.
    pub(super) fn run_op(&mut self, op: BridgeOp, cx: &mut Context<Self>) {
        let Some(page) = self.page.clone() else {
            self.answer(op.op_id, Err("browser is not open".into()));
            return;
        };
        match op.kind {
            BridgeOpKind::Eval { script } => {
                let (reply, answer) = futures::channel::oneshot::channel();
                page.evaluate(&script, reply);
                let outbox = self.bridge.as_ref().map(|bridge| bridge.outbox.clone());
                cx.spawn(async move |_, _| {
                    let result = answer
                        .await
                        .map_err(|_| "browser page script was cancelled".to_string());
                    if let Some(outbox) = outbox {
                        let _ = outbox.send(Outgoing::Reply(op.op_id, result));
                    }
                })
                .detach();
            }
            BridgeOpKind::EvalDetached { script } => {
                let (reply, _) = futures::channel::oneshot::channel();
                page.evaluate(&script, reply);
            }
            BridgeOpKind::Navigate { url } => {
                if let Ok(url) = validate_browser_url(&url) {
                    self.loading = true;
                    self.error = None;
                    self.set_address(url.as_str(), cx);
                    page.load_url(url.as_str());
                    cx.notify();
                }
            }
            BridgeOpKind::Reload => self.reload(cx),
            BridgeOpKind::Click { x, y } => {
                let delivered = page.click(x, y);
                self.answer(
                    op.op_id,
                    delivered
                        .then(|| "true".to_string())
                        .ok_or_else(|| "browser click could not be delivered".to_string()),
                );
            }
            BridgeOpKind::Press { key } => {
                let delivered = page.press(&key);
                self.answer(
                    op.op_id,
                    delivered
                        .then(|| "true".to_string())
                        .ok_or_else(|| "browser keyboard input could not be delivered".to_string()),
                );
            }
            BridgeOpKind::Snapshot => {
                let (reply, image) = futures::channel::oneshot::channel();
                page.snapshot(reply);
                let outbox = self.bridge.as_ref().map(|bridge| bridge.outbox.clone());
                cx.spawn(async move |_, _| {
                    use base64::Engine;
                    let result = image
                        .await
                        .unwrap_or_else(|_| Err("Browser snapshot was cancelled".into()))
                        .map(|png| base64::engine::general_purpose::STANDARD.encode(png));
                    if let Some(outbox) = outbox {
                        let _ = outbox.send(Outgoing::Reply(op.op_id, result));
                    }
                })
                .detach();
            }
        }
    }
}

/// Sends each message in order until the Browser closes or the desktop
/// goes away.
fn send_out(api: &DesktopApi, token: u64, inbox: mpsc::Receiver<Outgoing>) {
    for message in inbox {
        let sent = match &message {
            Outgoing::Report(report) => api.browser_report(token, report),
            Outgoing::Reply(op_id, result) => api.browser_reply(*op_id, result),
            Outgoing::Close => {
                let _ = api.browser_close(token);
                return;
            }
        };
        if matches!(sent, Err(crate::data::desktop_api::ApiError::DesktopClosed)) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fake_desktop_routes;

    #[test]
    fn reports_answers_and_the_close_go_out_in_order() {
        let (port, server) = fake_desktop_routes(vec![
            ("/api/v1/browser/bridge/surface/41/report", 200, "{}".into()),
            (
                "/api/v1/browser/bridge/ops/7/reply",
                200,
                r#"{"delivered":true}"#.into(),
            ),
            ("/api/v1/browser/bridge/surface/41/close", 200, "{}".into()),
        ]);
        let (outbox, inbox) = mpsc::channel();
        outbox
            .send(Outgoing::Report(PageReport::Loaded(
                "https://ex.com/".into(),
            )))
            .unwrap();
        outbox
            .send(Outgoing::Reply(7, Ok("\"Docs\"".into())))
            .unwrap();
        outbox.send(Outgoing::Close).unwrap();
        send_out(&DesktopApi::new(port, "k"), 41, inbox);
        let requests = server.join().unwrap();
        let lines: Vec<&str> = requests.iter().map(|(line, _)| line.as_str()).collect();
        assert_eq!(
            lines,
            [
                "POST /api/v1/browser/bridge/surface/41/report",
                "POST /api/v1/browser/bridge/ops/7/reply",
                "POST /api/v1/browser/bridge/surface/41/close",
            ]
        );
        let report: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
        assert_eq!(report["kind"], "loaded");
        let reply: serde_json::Value = serde_json::from_str(&requests[1].1).unwrap();
        assert_eq!(reply["raw"], "\"Docs\"");
    }
}
