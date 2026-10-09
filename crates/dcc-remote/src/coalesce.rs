//! Streaming text arrives as one `*Delta` event per token. Over the relay
//! the desktop merges deltas of the same item for a short window before
//! encrypting, which saves Mac CPU and the phone's battery and data.
//! Lifecycle events (a permission request, a question, the end of a turn)
//! never wait: they flush whatever is pending and go out right away, so
//! order is kept.

use std::time::{Duration, Instant};

use serde_json::Value;

pub const DEFAULT_WINDOW: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq)]
pub struct OutgoingEvent {
    pub event: String,
    pub data: Value,
    pub seq: Option<u64>,
}

/// `core-event` variants that carry a `content` fragment of one item, and
/// the field naming that item.
const DELTA_VARIANTS: &[(&str, &str)] = &[
    ("sessionTurnDelta", "turn_id"),
    ("sessionTurnAssistantMessageDelta", "message_id"),
    ("sessionTurnReasoningDelta", "reasoning_id"),
    ("sessionTurnToolCallDelta", "tool_call_id"),
    ("sessionDelegationDelta", "delegation_id"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeltaKey {
    variant: &'static str,
    session_id: String,
    item_id: String,
}

fn delta_key(event: &OutgoingEvent) -> Option<DeltaKey> {
    if event.event != "core-event" {
        return None;
    }
    let object = event.data.as_object()?;
    if object.len() != 1 {
        return None;
    }
    let (variant, inner) = object.iter().next()?;
    let (variant, id_field) = DELTA_VARIANTS
        .iter()
        .find(|(name, _)| *name == variant.as_str())?;
    inner.get("content")?.as_str()?;
    Some(DeltaKey {
        variant,
        session_id: inner.get("session_id")?.as_str()?.to_string(),
        item_id: inner.get(*id_field)?.as_str()?.to_string(),
    })
}

fn append_content(into: &mut OutgoingEvent, from: &OutgoingEvent, variant: &str) {
    let addition = from.data[variant]["content"].as_str().unwrap_or_default();
    if let Some(Value::String(content)) = into
        .data
        .get_mut(variant)
        .and_then(|inner| inner.get_mut("content"))
    {
        content.push_str(addition);
    }
    into.seq = match (into.seq, from.seq) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
}

pub struct DeltaCoalescer {
    window: Duration,
    pending: Vec<(DeltaKey, OutgoingEvent)>,
    opened_at: Option<Instant>,
}

impl DeltaCoalescer {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            pending: Vec::new(),
            opened_at: None,
        }
    }

    /// Takes the next event and returns what should be sent now, in order.
    pub fn push(&mut self, event: OutgoingEvent, now: Instant) -> Vec<OutgoingEvent> {
        let Some(key) = delta_key(&event) else {
            let mut out = self.flush();
            out.push(event);
            return out;
        };
        match self.pending.iter_mut().find(|(pending, _)| *pending == key) {
            Some((_, merged)) => append_content(merged, &event, key.variant),
            None => self.pending.push((key, event)),
        }
        let opened_at = *self.opened_at.get_or_insert(now);
        if now >= opened_at + self.window {
            return self.flush();
        }
        Vec::new()
    }

    /// When the pending deltas must go out, if any are waiting.
    pub fn deadline(&self) -> Option<Instant> {
        self.opened_at.map(|opened_at| opened_at + self.window)
    }

    pub fn flush(&mut self) -> Vec<OutgoingEvent> {
        self.opened_at = None;
        self.pending.drain(..).map(|(_, event)| event).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn delta(message: &str, content: &str, seq: u64) -> OutgoingEvent {
        OutgoingEvent {
            event: "core-event".into(),
            data: json!({"sessionTurnAssistantMessageDelta": {
                "session_id": "s1", "turn_id": "t1", "message_id": message, "content": content
            }}),
            seq: Some(seq),
        }
    }

    #[test]
    fn merges_deltas_of_the_same_item_within_the_window() {
        let start = Instant::now();
        let mut coalescer = DeltaCoalescer::new(DEFAULT_WINDOW);
        assert!(coalescer.push(delta("m1", "Hel", 1), start).is_empty());
        assert!(coalescer
            .push(delta("m1", "lo", 2), start + Duration::from_millis(20))
            .is_empty());
        assert!(coalescer
            .push(delta("m2", "x", 3), start + Duration::from_millis(30))
            .is_empty());
        assert_eq!(coalescer.deadline(), Some(start + DEFAULT_WINDOW));
        let out = coalescer.flush();
        assert_eq!(out, vec![delta("m1", "Hello", 2), delta("m2", "x", 3)]);
        assert_eq!(coalescer.deadline(), None);
    }

    #[test]
    fn lifecycle_events_flush_pending_deltas_first() {
        let start = Instant::now();
        let mut coalescer = DeltaCoalescer::new(DEFAULT_WINDOW);
        coalescer.push(delta("m1", "a", 1), start);
        let permission = OutgoingEvent {
            event: "core-event".into(),
            data: json!({"sessionTurnPermissionRequested": {"session_id": "s1"}}),
            seq: Some(2),
        };
        let out = coalescer.push(permission.clone(), start + Duration::from_millis(1));
        assert_eq!(out, vec![delta("m1", "a", 1), permission]);
    }

    #[test]
    fn a_delta_past_the_window_flushes_everything() {
        let start = Instant::now();
        let mut coalescer = DeltaCoalescer::new(DEFAULT_WINDOW);
        coalescer.push(delta("m1", "a", 1), start);
        let out = coalescer.push(delta("m1", "b", 2), start + Duration::from_millis(200));
        assert_eq!(out, vec![delta("m1", "ab", 2)]);
    }
}
