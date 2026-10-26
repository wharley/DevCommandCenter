//! Server-sent events from the desktop's API, read on a blocking thread:
//! the response body with HTTP/1.1 chunks taken off, and its frames
//! (`event:` and `data:` lines, a blank line closing each).

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;

/// The response body, with HTTP/1.1 chunks taken off.
pub(crate) struct Body {
    reader: BufReader<TcpStream>,
    chunked: bool,
    /// Bytes left in the chunk being read.
    left: usize,
}

impl Body {
    pub(crate) fn new(reader: BufReader<TcpStream>, chunked: bool) -> Self {
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

/// The frames, `(event, data)`, as whole lines arrive.
#[derive(Default)]
pub(crate) struct SseFrames {
    pending: Vec<u8>,
    name: String,
    data: String,
    ready: std::collections::VecDeque<(String, String)>,
}

impl SseFrames {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                let name = std::mem::take(&mut self.name);
                let data = std::mem::take(&mut self.data);
                if !name.is_empty() || !data.is_empty() {
                    self.ready.push_back((name, data));
                }
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

    pub(crate) fn next_frame(&mut self) -> Option<(String, String)> {
        self.ready.pop_front()
    }
}
