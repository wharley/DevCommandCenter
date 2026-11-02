//! One opened simulator: the Claude helper's `device` subcommand runs it in
//! its own process, so a crash in the private simulator frameworks never
//! takes the app down. The app listens on a Unix socket in a private folder
//! (0700) under `/tmp` — a socket path holds 104 bytes at most, too few for
//! `$TMPDIR` — and the helper connects; frames and replies come back on it,
//! commands go as JSON lines. Dropping the link ends the helper.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::channel::{mpsc, oneshot};
use serde_json::{Value, json};

use super::stream::{Chunk, Reader};

/// How long the helper has to start and connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub enum LinkEvent {
    Description(Vec<u8>),
    Frame {
        keyframe: bool,
        data: Vec<u8>,
    },
    /// The screen's size in pixels.
    Size(u32, u32),
    /// The helper is gone (it ended, or the stream broke).
    Closed(Option<String>),
}

type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct DeviceLink {
    child: Mutex<Child>,
    writer: Mutex<UnixStream>,
    next_id: AtomicU64,
    waiting: Waiting,
    folder: PathBuf,
}

fn private_folder() -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let folder = PathBuf::from(format!(
        "/tmp/dcc-dev-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir(&folder).map_err(|error| error.to_string())?;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    Ok(folder)
}

fn accept(listener: &UnixListener, child: &mut Child) -> Result<UnixStream, String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .map_err(|error| error.to_string())?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.to_string()),
        }
        if let Ok(Some(status)) = child.try_wait() {
            let mut log = String::new();
            if let Some(mut output) = child.stdout.take() {
                let _ = output.read_to_string(&mut log);
            }
            return Err(format!(
                "the simulator helper ended ({status}): {}",
                log.trim()
            ));
        }
        if Instant::now() > deadline {
            return Err("the simulator helper did not start".into());
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

impl DeviceLink {
    /// Starts the helper for the booted simulator `udid`. Blocks until it
    /// connects (run it off the main thread).
    pub fn open(
        helper: &Path,
        udid: &str,
    ) -> Result<(Self, mpsc::UnboundedReceiver<LinkEvent>), String> {
        let folder = private_folder()?;
        let socket = folder.join("s");
        let listener = UnixListener::bind(&socket).map_err(|error| error.to_string())?;
        let mut child = Command::new(helper)
            .args(["device", "--udid", udid, "--socket"])
            .arg(&socket)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("{}: {error}", helper.display()))?;
        let stream = match accept(&listener, &mut child) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = child.kill();
                let _ = std::fs::remove_dir_all(&folder);
                return Err(error);
            }
        };
        // The addon's own logs; read so the pipe never fills.
        for pipe in [
            child
                .stdout
                .take()
                .map(|out| Box::new(out) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|err| Box::new(err) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            std::thread::spawn(move || {
                let mut pipe = pipe;
                let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            });
        }
        let (events, receiver) = mpsc::unbounded();
        let waiting: Waiting = Arc::default();
        let reader = stream.try_clone().map_err(|error| error.to_string())?;
        let replies = waiting.clone();
        std::thread::spawn(move || read_loop(reader, events, replies));
        Ok((
            Self {
                child: Mutex::new(child),
                writer: Mutex::new(stream),
                next_id: AtomicU64::new(1),
                waiting,
                folder,
            },
            receiver,
        ))
    }

    fn write(&self, mut command: Value, id: u64) -> Result<(), String> {
        command["id"] = json!(id);
        let mut line = serde_json::to_vec(&command).map_err(|error| error.to_string())?;
        line.push(b'\n');
        self.writer
            .lock()
            .map_err(|_| "the simulator link is broken".to_string())?
            .write_all(&line)
            .map_err(|error| error.to_string())
    }

    /// A command whose answer nobody waits for (a touch, a key).
    pub fn send(&self, command: Value) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.write(command, id);
    }

    /// A command and its answer.
    pub fn request(&self, command: Value) -> oneshot::Receiver<Result<Value, String>> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.insert(id, reply);
        }
        if let Err(error) = self.write(command, id)
            && let Some(reply) = self.waiting.lock().ok().and_then(|mut w| w.remove(&id))
        {
            let _ = reply.send(Err(error));
        }
        answer
    }
}

fn read_loop(mut stream: UnixStream, events: mpsc::UnboundedSender<LinkEvent>, waiting: Waiting) {
    let mut reader = Reader::default();
    let mut buffer = vec![0u8; 256 * 1024];
    let reason = loop {
        let read = match stream.read(&mut buffer) {
            Ok(0) => break None,
            Ok(read) => read,
            Err(error) => break Some(error.to_string()),
        };
        let chunks = match reader.push(&buffer[..read]) {
            Ok(chunks) => chunks,
            Err(error) => break Some(error),
        };
        for chunk in chunks {
            let event = match chunk {
                Chunk::Description(data) => LinkEvent::Description(data),
                Chunk::Frame { keyframe, data } => LinkEvent::Frame { keyframe, data },
                Chunk::Status(status) => {
                    match (status["width"].as_u64(), status["height"].as_u64()) {
                        (Some(width), Some(height)) => LinkEvent::Size(width as u32, height as u32),
                        _ => continue,
                    }
                }
                Chunk::Reply(reply) => {
                    let id = reply["id"].as_u64();
                    let sender = id.and_then(|id| waiting.lock().ok()?.remove(&id));
                    if let Some(sender) = sender {
                        let _ = sender.send(if reply["ok"].as_bool() == Some(true) {
                            Ok(reply["result"].clone())
                        } else {
                            Err(reply["error"].as_str().unwrap_or("failed").to_string())
                        });
                    }
                    continue;
                }
            };
            if events.unbounded_send(event).is_err() {
                return;
            }
        }
    };
    let _ = events.unbounded_send(LinkEvent::Closed(reason));
}

impl Drop for DeviceLink {
    fn drop(&mut self) {
        if let Ok(writer) = self.writer.lock() {
            let _ = writer.shutdown(std::net::Shutdown::Both);
        }
        if let Ok(mut child) = self.child.lock() {
            // It ends on the socket closing; a stuck one is killed.
            let deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in helper: connects, says its size, sends a frame, answers
    /// one command, then waits for the socket to close.
    #[test]
    fn a_helper_streams_answers_and_ends_with_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("helper.py");
        std::fs::write(
            &script,
            r#"#!/usr/bin/env python3
import json, socket, sys
path = sys.argv[sys.argv.index("--socket") + 1]
s = socket.socket(socket.AF_UNIX); s.connect(path)
def send(tag, payload):
    s.sendall((len(payload) + 1).to_bytes(4, "big") + bytes([tag]) + payload)
print("[capture] logs go to stdout", flush=True)
send(6, json.dumps({"width": 1206, "height": 2622}).encode())
send(2, b"\x00\x00\x00\x01\x65")
line = b""
while not line.endswith(b"\n"):
    part = s.recv(1024)
    if not part: sys.exit(0)
    line += part
command = json.loads(line)
send(5, json.dumps({"id": command["id"], "ok": True, "result": command["op"]}).encode())
while s.recv(1024): pass
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (link, mut events) =
            DeviceLink::open(&script, "47B9DDB6-8CF6-4866-9A60-B4D1F6EC0B4D").unwrap();
        let folder = link.folder.clone();
        let answer = link.request(json!({"op": "describe"}));
        let answer = futures::executor::block_on(answer).unwrap();
        assert_eq!(answer, Ok(json!("describe")));
        let first = futures::executor::block_on(futures::StreamExt::next(&mut events));
        assert!(
            matches!(first, Some(LinkEvent::Size(1206, 2622))),
            "{first:?}"
        );
        let second = futures::executor::block_on(futures::StreamExt::next(&mut events));
        assert!(matches!(
            second,
            Some(LinkEvent::Frame { keyframe: true, .. })
        ));
        assert!(folder.exists());
        drop(link);
        assert!(!folder.exists(), "the private folder goes with the link");
    }
}
