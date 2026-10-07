//! Merges bursts of streamed provider deltas before they reach the durable
//! session log. Every event the bridge receives costs one SQLite transaction
//! and several webview emits, so token-level deltas used to dominate the log
//! (the vast majority of rows in long sessions). Deltas for the same item that
//! arrive within a short window become one event; any other event closes the
//! window immediately, so ordering and terminal latency are unchanged.

use std::time::Duration;

use dcc_core::{domain::provider::ProviderEvent, Result};
use futures::stream::{self, BoxStream, StreamExt};
use tokio::time::{timeout_at, Instant};

pub const DEFAULT_DELTA_WINDOW: Duration = Duration::from_millis(50);
const MAX_MERGED_DELTA_BYTES: usize = 16 * 1024;

#[derive(PartialEq, Eq)]
enum DeltaKey<'a> {
    Text,
    Assistant(&'a str),
    Reasoning(&'a str),
    Tool(&'a str),
}

fn delta_key(event: &ProviderEvent) -> Option<DeltaKey<'_>> {
    match event {
        ProviderEvent::TextDelta { .. } => Some(DeltaKey::Text),
        ProviderEvent::AssistantMessageDelta { id, .. } => Some(DeltaKey::Assistant(id)),
        ProviderEvent::ReasoningDelta { id, .. } => Some(DeltaKey::Reasoning(id)),
        ProviderEvent::ToolCallDelta { id, .. } => Some(DeltaKey::Tool(id)),
        _ => None,
    }
}

fn delta_content_mut(event: &mut ProviderEvent) -> Option<&mut String> {
    match event {
        ProviderEvent::TextDelta { content }
        | ProviderEvent::AssistantMessageDelta { content, .. }
        | ProviderEvent::ReasoningDelta { content, .. }
        | ProviderEvent::ToolCallDelta { content, .. } => Some(content),
        _ => None,
    }
}

/// Appends `next` into `acc` when both are deltas of the same item.
fn try_merge(acc: &mut ProviderEvent, next: &ProviderEvent) -> bool {
    let same = match (delta_key(acc), delta_key(next)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    };
    if !same {
        return false;
    }
    let addition = match next {
        ProviderEvent::TextDelta { content }
        | ProviderEvent::AssistantMessageDelta { content, .. }
        | ProviderEvent::ReasoningDelta { content, .. }
        | ProviderEvent::ToolCallDelta { content, .. } => content.as_str(),
        _ => return false,
    };
    let Some(content) = delta_content_mut(acc) else {
        return false;
    };
    if content.len() + addition.len() > MAX_MERGED_DELTA_BYTES {
        return false;
    }
    content.push_str(addition);
    true
}

struct CoalesceState {
    inner: BoxStream<'static, Result<ProviderEvent>>,
    lookahead: Option<Result<ProviderEvent>>,
    finished: bool,
    window: Duration,
}

pub fn coalesce_provider_deltas(
    inner: BoxStream<'static, Result<ProviderEvent>>,
    window: Duration,
) -> BoxStream<'static, Result<ProviderEvent>> {
    let state = CoalesceState {
        inner,
        lookahead: None,
        finished: false,
        window,
    };
    stream::unfold(state, |mut state| async move {
        let first = match state.lookahead.take() {
            Some(item) => item,
            None if state.finished => return None,
            None => state.inner.next().await?,
        };
        let mut acc = match first {
            Ok(event) if delta_key(&event).is_some() => event,
            other => return Some((other, state)),
        };
        let deadline = Instant::now() + state.window;
        loop {
            match timeout_at(deadline, state.inner.next()).await {
                Err(_elapsed) => break,
                Ok(None) => {
                    state.finished = true;
                    break;
                }
                Ok(Some(Ok(next))) if try_merge(&mut acc, &next) => {}
                Ok(Some(next)) => {
                    state.lookahead = Some(next);
                    break;
                }
            }
        }
        Some((Ok(acc), state))
    })
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;

    fn assistant(id: &str, content: &str) -> ProviderEvent {
        ProviderEvent::AssistantMessageDelta {
            id: id.to_string(),
            content: content.to_string(),
        }
    }

    fn collect(events: Vec<ProviderEvent>) -> Vec<ProviderEvent> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime");
        runtime.block_on(async move {
            let inner = stream::iter(events.into_iter().map(Ok)).boxed();
            coalesce_provider_deltas(inner, Duration::from_millis(50))
                .map(|event| event.expect("ok"))
                .collect::<Vec<_>>()
                .await
        })
    }

    #[test]
    fn merges_consecutive_deltas_of_the_same_message() {
        let merged = collect(vec![
            assistant("m1", "Hel"),
            assistant("m1", "lo "),
            assistant("m1", "world"),
        ]);
        assert_eq!(merged.len(), 1);
        assert!(
            matches!(&merged[0], ProviderEvent::AssistantMessageDelta { content, .. } if content == "Hello world")
        );
    }

    #[test]
    fn keeps_order_and_never_merges_across_items_or_other_events() {
        let merged = collect(vec![
            assistant("m1", "a"),
            ProviderEvent::ReasoningDelta {
                id: "r1".to_string(),
                content: "think".to_string(),
            },
            assistant("m1", "b"),
            assistant("m2", "c"),
            ProviderEvent::Completed {
                at: "now".to_string(),
            },
        ]);
        assert_eq!(merged.len(), 5);
        assert!(matches!(&merged[0], ProviderEvent::AssistantMessageDelta { content, .. } if content == "a"));
        assert!(matches!(&merged[1], ProviderEvent::ReasoningDelta { .. }));
        assert!(matches!(&merged[3], ProviderEvent::AssistantMessageDelta { id, .. } if id == "m2"));
        assert!(matches!(&merged[4], ProviderEvent::Completed { .. }));
    }

    #[test]
    fn flushes_a_lone_delta_once_the_window_elapses() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let (tx, rx) = mpsc::unbounded::<Result<ProviderEvent>>();
            let mut coalesced = coalesce_provider_deltas(rx.boxed(), Duration::from_millis(20));
            tx.unbounded_send(Ok(assistant("m1", "first"))).expect("send");
            // The sender stays open: the delta must still come out.
            let first = tokio::time::timeout(Duration::from_secs(2), coalesced.next())
                .await
                .expect("flushed by window")
                .expect("item")
                .expect("ok");
            assert!(matches!(first, ProviderEvent::AssistantMessageDelta { content, .. } if content == "first"));
            tx.unbounded_send(Ok(assistant("m1", "second"))).expect("send");
            drop(tx);
            let rest = coalesced.collect::<Vec<_>>().await;
            assert_eq!(rest.len(), 1);
        });
    }

    #[test]
    fn caps_merged_delta_size() {
        let chunk = "x".repeat(10 * 1024);
        let merged = collect(vec![assistant("m1", &chunk), assistant("m1", &chunk)]);
        assert_eq!(merged.len(), 2);
    }
}
