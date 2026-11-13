//! A machine's terminals over the relay channel, for another DCC that runs a
//! task here (a phone never reaches them). The shells are this machine's own
//! PTYs, opened, written to and resized through the same API the app's dock
//! uses; what they print goes back as `evt` frames of a subscription to
//! `/api/v1/terminals/events/stream`.
//!
//! A channel only sees the terminals it opened: the output of this
//! machine's other shells (its person's own, another paired computer's)
//! never crosses, and it cannot write to or end them. A shell outlives the
//! channel that opened it; opening it again by its owner key attaches to it.

use std::collections::HashSet;
use std::sync::Mutex;

use dcc_remote::frames::{End, Frame, StreamEvent};
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;

use crate::http_api::{subscribe_desktop_terminal_events, DesktopTerminalEvent};

pub(crate) const STREAM_PATH: &str = "/api/v1/terminals/events/stream";
const OPEN_PREFIX: &str = "/api/v1/terminals/by-owner/";
const TERMINAL_PREFIX: &str = "/api/v1/terminals/";

/// The PTYs one channel opened.
#[derive(Default)]
pub(crate) struct ChannelTerminals(Mutex<HashSet<String>>);

impl ChannelTerminals {
    fn opened(&self, pty_id: String) {
        if let Ok(mut owned) = self.0.lock() {
            owned.insert(pty_id);
        }
    }

    fn owns(&self, pty_id: &str) -> bool {
        self.0.lock().is_ok_and(|owned| owned.contains(pty_id))
    }

    /// A successful `by-owner` answer names the PTY this channel now owns.
    pub(crate) fn note_opened(&self, path: &str, status: u16, body: &[u8]) {
        if let Some(pty_id) = opened_pty(path, status, body) {
            self.opened(pty_id);
        }
    }

    /// Whether a request may go on: anything that is not one PTY's own
    /// route, or a PTY this channel opened.
    pub(crate) fn may_reach(&self, path: &str) -> bool {
        pty_of(path).is_none_or(|pty_id| self.owns(pty_id))
    }
}

fn route(path: &str) -> &str {
    path.split(['?', '#']).next().unwrap_or_default()
}

/// The PTY a `by-owner` call opened or attached to.
fn opened_pty(path: &str, status: u16, body: &[u8]) -> Option<String> {
    if !(200..300).contains(&status) || !route(path).starts_with(OPEN_PREFIX) {
        return None;
    }
    let answer: Value = serde_json::from_slice(body).ok()?;
    answer
        .get("ptyId")
        .and_then(Value::as_str)
        .filter(|pty_id| !pty_id.is_empty())
        .map(str::to_string)
}

/// The PTY of `/api/v1/terminals/<pty>/{write,resize,kill}`.
fn pty_of(path: &str) -> Option<&str> {
    let rest = route(path).strip_prefix(TERMINAL_PREFIX)?;
    let (pty_id, action) = rest.split_once('/')?;
    matches!(action, "write" | "resize" | "kill").then_some(pty_id)
}

fn pty_id(event: &DesktopTerminalEvent) -> &str {
    match event {
        DesktopTerminalEvent::Output { pty_id, .. } | DesktopTerminalEvent::Exit { pty_id, .. } => {
            pty_id
        }
    }
}

/// The event as the channel carries it; none when it is not this channel's.
fn frame(id: u64, terminals: &ChannelTerminals, event: &DesktopTerminalEvent) -> Option<Frame> {
    if !terminals.owns(pty_id(event)) {
        return None;
    }
    Some(Frame::Evt(StreamEvent {
        id,
        event: event.name().to_string(),
        data: serde_json::to_value(event).ok()?,
        seq: None,
    }))
}

/// Sends what this channel's terminals print until the subscription is
/// cancelled (the task is aborted) or the channel closes.
pub(crate) async fn stream(
    terminals: std::sync::Arc<ChannelTerminals>,
    id: u64,
    tx: mpsc::Sender<Frame>,
) {
    let mut events = subscribe_desktop_terminal_events();
    loop {
        match events.recv().await {
            Ok(event) => {
                let Some(frame) = frame(id, &terminals, &event) else {
                    continue;
                };
                if tx.send(frame).await.is_err() {
                    return;
                }
            }
            // Output was dropped: the other side opens its shells again,
            // which brings what they printed.
            Err(RecvError::Lagged(_)) => {
                let _ = tx
                    .send(Frame::End(End {
                        id,
                        error: Some("terminal output was dropped".into()),
                    }))
                    .await;
                return;
            }
            Err(RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_reaches_only_the_terminals_it_opened() {
        let terminals = ChannelTerminals::default();
        // Opening is free; a PTY's own routes need it opened here.
        assert!(terminals.may_reach("/api/v1/terminals/by-owner/terminal%3Aworktree%3Aw-1%3A0"));
        assert!(terminals.may_reach("/api/v1/terminals/activity"));
        assert!(!terminals.may_reach("/api/v1/terminals/pty-9/write"));
        terminals.note_opened(
            "/api/v1/terminals/by-owner/terminal%3Aworktree%3Aw-1%3A0",
            200,
            br#"{"ptyId":"pty-1","existing":false}"#,
        );
        assert!(terminals.may_reach("/api/v1/terminals/pty-1/write"));
        assert!(terminals.may_reach("/api/v1/terminals/pty-1/resize?x=1"));
        assert!(!terminals.may_reach("/api/v1/terminals/pty-9/kill"));
        // A failed opening, or another route's answer, owns nothing.
        terminals.note_opened("/api/v1/terminals/by-owner/x", 500, br#"{"ptyId":"pty-7"}"#);
        terminals.note_opened(
            "/api/v1/terminals/pty-1/write",
            200,
            br#"{"ptyId":"pty-8"}"#,
        );
        assert!(!terminals.may_reach("/api/v1/terminals/pty-7/write"));
        assert!(!terminals.may_reach("/api/v1/terminals/pty-8/write"));
    }

    #[test]
    fn only_this_channels_output_becomes_a_frame() {
        let terminals = ChannelTerminals::default();
        terminals.opened("pty-1".into());
        let mine = DesktopTerminalEvent::Output {
            pty_id: "pty-1".into(),
            data: "ok\r\n".into(),
        };
        let Some(Frame::Evt(event)) = frame(4, &terminals, &mine) else {
            panic!("a frame");
        };
        assert_eq!((event.id, event.event.as_str()), (4, "terminal-output"));
        assert_eq!(event.data["ptyId"], "pty-1");
        assert_eq!(event.data["data"], "ok\r\n");
        let other = DesktopTerminalEvent::Output {
            pty_id: "pty-2".into(),
            data: "segredo".into(),
        };
        assert!(frame(4, &terminals, &other).is_none());
    }
}
