//! The desktop's terminals (`/api/v1/terminals/*`): the shells live in the
//! desktop's process, the same ones its dock attaches to, and the native
//! dock reads what they print from one event stream. Blocking: call it off
//! the UI thread.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::data::desktop_api::{ApiError, DesktopApi, encode};

/// The shell a tab runs and the size it starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellSpec {
    pub cwd: String,
    pub command: String,
    pub args: Vec<String>,
    pub cols: u16,
    pub rows: u16,
}

impl ShellSpec {
    /// `getDefaultShell` and the login flag the dock adds for zsh, bash and sh.
    pub fn login(cwd: String, shell: Option<String>, cols: u16, rows: u16) -> Self {
        let command = shell
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|| "/bin/zsh".to_string());
        let name = command.rsplit(['/', '\\']).next().unwrap_or_default();
        let args = if matches!(name, "zsh" | "bash" | "sh") {
            vec!["-l".to_string()]
        } else {
            Vec::new()
        };
        Self {
            cwd,
            command,
            args,
            cols,
            rows,
        }
    }
}

/// `terminal_get_or_create`'s answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedTerminal {
    pub pty_id: String,
    /// The owner's shell was already running.
    pub existing: bool,
    /// What it printed so far, in order.
    pub chunks: Vec<String>,
    pub exited: bool,
    pub exit_code: Option<i64>,
}

/// `TerminalRuntimeActivityStatus`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityStatus {
    #[default]
    Idle,
    Running,
    Waiting,
    Exited,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalActivity {
    pub pty_id: String,
    #[serde(default)]
    pub status: ActivityStatus,
    /// The process in front (`npm run dev`), while one runs.
    #[serde(default)]
    pub process_label: Option<String>,
}

/// One message of the event stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    Output {
        pty_id: String,
        data: String,
    },
    Exit {
        pty_id: String,
        code: Option<i64>,
    },
    /// The stream fell behind: what each terminal shows is read again.
    Lagged,
}

fn opened(value: Value) -> Result<OpenedTerminal, ApiError> {
    let pty_id = value
        .get("ptyId")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::Failed("no terminal".into()))?
        .to_string();
    let session = value.get("session");
    Ok(OpenedTerminal {
        pty_id,
        existing: value
            .get("existing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        chunks: value
            .get("chunks")
            .and_then(Value::as_array)
            .map(|chunks| {
                chunks
                    .iter()
                    .filter_map(|chunk| chunk.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        exited: session
            .and_then(|session| session.get("status"))
            .and_then(Value::as_str)
            == Some("exited"),
        exit_code: session
            .and_then(|session| session.get("lastExitCode"))
            .and_then(Value::as_i64),
    })
}

fn ok(value: Value) -> bool {
    value.get("ok").and_then(Value::as_bool).unwrap_or(false)
}

impl DesktopApi {
    /// The owner's shell (`terminal:<scope>:<tab>`), started when it has none.
    pub fn open_terminal(
        &self,
        owner_key: &str,
        spec: &ShellSpec,
    ) -> Result<OpenedTerminal, ApiError> {
        opened(self.post(
            &format!("/api/v1/terminals/by-owner/{}", encode(owner_key)),
            &json!({
                "cwd": spec.cwd,
                "command": spec.command,
                "args": spec.args,
                "cols": spec.cols,
                "rows": spec.rows,
            }),
        )?)
    }

    pub fn write_terminal(&self, pty_id: &str, data: &str) -> Result<bool, ApiError> {
        self.post(
            &format!("/api/v1/terminals/{}/write", encode(pty_id)),
            &json!({ "data": data }),
        )
        .map(ok)
    }

    pub fn resize_terminal(&self, pty_id: &str, cols: u16, rows: u16) -> Result<bool, ApiError> {
        self.post(
            &format!("/api/v1/terminals/{}/resize", encode(pty_id)),
            &json!({ "cols": cols, "rows": rows }),
        )
        .map(ok)
    }

    pub fn kill_terminal(&self, pty_id: &str) -> Result<bool, ApiError> {
        self.post(
            &format!("/api/v1/terminals/{}/kill", encode(pty_id)),
            &json!({}),
        )
        .map(ok)
    }

    pub fn terminal_activity(&self) -> Result<Vec<TerminalActivity>, ApiError> {
        let value = self.get("/api/v1/terminals/activity")?;
        Ok(value
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Every terminal's output and end, until the desktop closes.
    pub fn terminal_events(&self) -> Result<TerminalEvents, ApiError> {
        let (reader, chunked) =
            self.open_stream("/api/v1/terminals/events/stream", Duration::from_secs(45))?;
        Ok(TerminalEvents {
            body: Body::new(reader, chunked),
            decoder: SseDecoder::default(),
        })
    }
}

/// The response body, with HTTP/1.1 chunks taken off.
struct Body {
    reader: BufReader<TcpStream>,
    chunked: bool,
    /// Bytes left in the chunk being read.
    left: usize,
}

impl Body {
    fn new(reader: BufReader<TcpStream>, chunked: bool) -> Self {
        Self {
            reader,
            chunked,
            left: 0,
        }
    }
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.chunked {
            return self.reader.read(buf);
        }
        if self.left == 0 {
            let mut line = String::new();
            // The CRLF closing the previous chunk, then the size line.
            while line.trim().is_empty() {
                line.clear();
                if self.reader.read_line(&mut line)? == 0 {
                    return Ok(0);
                }
            }
            let size = line.split(';').next().unwrap_or_default().trim();
            self.left =
                usize::from_str_radix(size, 16).map_err(|_| std::io::Error::other("bad chunk"))?;
            if self.left == 0 {
                return Ok(0);
            }
        }
        let wanted = buf.len().min(self.left);
        let read = self.reader.read(&mut buf[..wanted])?;
        self.left -= read;
        Ok(read)
    }
}

/// The stream's messages, one at a time; `None` once it ends (the desktop
/// closed, or nothing came for longer than its keep-alive).
pub struct TerminalEvents {
    body: Body,
    decoder: SseDecoder,
}

impl Iterator for TerminalEvents {
    type Item = TerminalEvent;

    fn next(&mut self) -> Option<TerminalEvent> {
        let mut buffer = [0u8; 16 * 1024];
        loop {
            if let Some(event) = self.decoder.next_event() {
                return Some(event);
            }
            match self.body.read(&mut buffer) {
                Ok(0) | Err(_) => return None,
                Ok(read) => self.decoder.feed(&buffer[..read]),
            }
        }
    }
}

/// Server-sent events into terminal events (`event:` and `data:` lines, a
/// blank line closing each).
#[derive(Default)]
pub(crate) struct SseDecoder {
    pending: Vec<u8>,
    name: String,
    data: String,
    ready: std::collections::VecDeque<TerminalEvent>,
}

impl SseDecoder {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                self.dispatch();
            } else if let Some(name) = line.strip_prefix("event:") {
                self.name = name.trim_start().to_string();
            } else if let Some(data) = line.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
            }
        }
    }

    pub(crate) fn next_event(&mut self) -> Option<TerminalEvent> {
        self.ready.pop_front()
    }

    fn dispatch(&mut self) {
        let name = std::mem::take(&mut self.name);
        let data = std::mem::take(&mut self.data);
        let value: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let event = match name.as_str() {
            "terminal-output" => text("ptyId")
                .zip(text("data"))
                .map(|(pty_id, data)| TerminalEvent::Output { pty_id, data }),
            "terminal-exit" => text("ptyId").map(|pty_id| TerminalEvent::Exit {
                pty_id,
                code: value.get("code").and_then(Value::as_i64),
            }),
            "terminal-lagged" => Some(TerminalEvent::Lagged),
            _ => None,
        };
        self.ready.extend(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_flag_goes_to_the_usual_shells() {
        let spec = ShellSpec::login("/repo".into(), Some("/bin/bash".into()), 120, 32);
        assert_eq!(spec.args, vec!["-l".to_string()]);
        let fish = ShellSpec::login("/repo".into(), Some("/opt/bin/fish".into()), 120, 32);
        assert!(fish.args.is_empty());
        let none = ShellSpec::login("/repo".into(), None, 120, 32);
        assert_eq!(none.command, "/bin/zsh");
    }

    #[test]
    fn events_split_across_reads_come_out_whole() {
        let mut decoder = SseDecoder::default();
        decoder.feed(b"event: terminal-output\ndata: {\"kind\":\"output\",\"pty");
        assert_eq!(decoder.next_event(), None);
        decoder.feed(b"Id\":\"pty-1\",\"data\":\"hi\\r\\n\"}\n\n:keep-alive\n\n");
        decoder.feed(b"event: terminal-exit\r\ndata: {\"ptyId\":\"pty-1\",\"code\":0}\r\n\r\n");
        decoder.feed(b"event: terminal-lagged\ndata: {}\n\n");
        assert_eq!(
            decoder.next_event(),
            Some(TerminalEvent::Output {
                pty_id: "pty-1".into(),
                data: "hi\r\n".into(),
            })
        );
        assert_eq!(
            decoder.next_event(),
            Some(TerminalEvent::Exit {
                pty_id: "pty-1".into(),
                code: Some(0),
            })
        );
        assert_eq!(decoder.next_event(), Some(TerminalEvent::Lagged));
        assert_eq!(decoder.next_event(), None);
    }

    #[test]
    fn the_stream_reads_chunked_events_until_it_ends() {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
            }
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n"
            )
            .unwrap();
            let events = [
                "event: terminal-output\ndata: {\"kind\":\"output\",\"ptyId\":\"p\",\"data\":\"a\"}\n\n",
                ":keep-alive\n\n",
                "event: terminal-exit\ndata: {\"kind\":\"exit\",\"ptyId\":\"p\",\"code\":1}\n\n",
            ];
            for event in events {
                // Split mid-event, as the network may.
                let (head, tail) = event.split_at(event.len() / 2);
                for part in [head, tail] {
                    write!(stream, "{:x}\r\n{part}\r\n", part.len()).unwrap();
                    stream.flush().unwrap();
                }
            }
            write!(stream, "0\r\n\r\n").unwrap();
            request
        });
        let events: Vec<_> = DesktopApi::new(port, "k")
            .terminal_events()
            .unwrap()
            .collect();
        assert_eq!(
            server.join().unwrap(),
            "GET /api/v1/terminals/events/stream HTTP/1.1\r\n"
        );
        assert_eq!(
            events,
            vec![
                TerminalEvent::Output {
                    pty_id: "p".into(),
                    data: "a".into(),
                },
                TerminalEvent::Exit {
                    pty_id: "p".into(),
                    code: Some(1),
                },
            ]
        );
    }

    #[test]
    fn an_opened_terminal_reads_its_session() {
        let opened = opened(json!({
            "ptyId": "pty-4",
            "existing": true,
            "session": { "status": "exited", "lastExitCode": 2 },
            "chunks": ["a", "b"],
        }))
        .unwrap();
        assert_eq!(
            opened,
            OpenedTerminal {
                pty_id: "pty-4".into(),
                existing: true,
                chunks: vec!["a".into(), "b".into()],
                exited: true,
                exit_code: Some(2),
            }
        );
    }
}
