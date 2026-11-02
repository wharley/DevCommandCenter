//! Where the backend's events go: the core events (`EventBus`) and the
//! app-level ones by name (a forge metadata refresh, a review job). Inside
//! the desktop both reach its webview; without Tauri the host passes its own.

use std::sync::Arc;

use async_trait::async_trait;
use dcc_core::domain::session::SessionEventRecord;
use dcc_core::ports::events::{CoreEvent, SessionLiveEventEnvelope};
use dcc_core::ports::EventBus;
use serde::Serialize;

/// Receives an app-level event: its name and its JSON payload.
pub trait AppEmitter: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AppEvents {
    bus: Arc<dyn EventBus>,
    emitter: Arc<dyn AppEmitter>,
}

impl AppEvents {
    pub fn new(bus: Arc<dyn EventBus>, emitter: Arc<dyn AppEmitter>) -> Self {
        Self { bus, emitter }
    }

    /// Nowhere: a host whose windows read the API instead of events.
    pub fn silent() -> Self {
        Self::new(Arc::new(Silent), Arc::new(Silent))
    }

    pub fn emit<T: Serialize>(&self, event: &str, payload: T) -> Result<(), String> {
        let payload = serde_json::to_value(payload).map_err(|error| error.to_string())?;
        self.emitter.emit(event, payload)
    }
}

#[async_trait]
impl EventBus for AppEvents {
    async fn publish(&self, event: CoreEvent) -> dcc_core::Result<()> {
        self.bus.publish(event).await
    }

    async fn publish_durable_session(
        &self,
        record: &SessionEventRecord,
        event: CoreEvent,
    ) -> dcc_core::Result<()> {
        self.bus.publish_durable_session(record, event).await
    }

    async fn publish_session_live(&self, event: SessionLiveEventEnvelope) -> dcc_core::Result<()> {
        self.bus.publish_session_live(event).await
    }
}

struct Silent;

impl AppEmitter for Silent {
    fn emit(&self, _event: &str, _payload: serde_json::Value) -> Result<(), String> {
        Ok(())
    }
}

#[async_trait]
impl EventBus for Silent {
    async fn publish(&self, _event: CoreEvent) -> dcc_core::Result<()> {
        Ok(())
    }
}
