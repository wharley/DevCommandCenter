//! Incremental parser for the desktop's own SSE responses, which the
//! connector turns into `evt` frames.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
}

#[derive(Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Option<String>,
    id: Option<String>,
}

/// A line longer than this without a newline ends the stream as malformed.
const MAX_LINE: usize = 8 * 1024 * 1024;

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds bytes and returns the events they complete.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, &'static str> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        let mut start = 0;
        while let Some(offset) = self.buffer[start..].iter().position(|&b| b == b'\n') {
            let end = start + offset;
            let mut line = &self.buffer[start..end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            let line = String::from_utf8_lossy(line).into_owned();
            start = end + 1;
            if let Some(event) = self.line(&line) {
                events.push(event);
            }
        }
        self.buffer.drain(..start);
        if self.buffer.len() > MAX_LINE {
            return Err("SSE line too long");
        }
        Ok(events)
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            let data = self.data.take();
            let event = self.event.take();
            let id = self.id.take();
            return data.map(|data| SseEvent { event, data, id });
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "id" => self.id = Some(value.to_string()),
            "data" => match self.data.as_mut() {
                Some(data) => {
                    data.push('\n');
                    data.push_str(value);
                }
                None => self.data = Some(value.to_string()),
            },
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_split_chunks_comments_and_multiline_data() {
        let mut parser = SseParser::new();
        assert!(parser
            .feed(b": keep-alive\n\nevent: core-ev")
            .unwrap()
            .is_empty());
        let events = parser
            .feed(b"ent\r\nid: 7\ndata: {\"a\":1}\n\ndata: x\ndata: y\n\n")
            .unwrap();
        assert_eq!(
            events,
            vec![
                SseEvent {
                    event: Some("core-event".into()),
                    data: "{\"a\":1}".into(),
                    id: Some("7".into()),
                },
                SseEvent {
                    event: None,
                    data: "x\ny".into(),
                    id: None,
                },
            ]
        );
    }
}
