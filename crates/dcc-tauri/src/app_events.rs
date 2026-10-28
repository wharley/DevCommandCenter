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

    /// The desktop's webview, through the Tauri app.
    #[cfg(feature = "tauri")]
    pub fn tauri(app: tauri::AppHandle) -> Self {
        Self::new(
            Arc::new(crate::events::TauriEventBus::new(app.clone())),
            Arc::new(TauriEmitter(app)),
        )
    }

    /// Nowhere: a host whose windows read the API instead of events.
    pub fn silent() -> Self {
        Self::new(Arc::new(Silent), Arc::new(Silent))
    }

    /// Events by name to the desktop's webview, alone.
    #[cfg(feature = "tauri")]
    pub fn tauri_emitter(app: tauri::AppHandle) -> Arc<dyn AppEmitter> {
        Arc::new(TauriEmitter(app))
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

#[cfg(feature = "tauri")]
struct TauriEmitter(tauri::AppHandle);

#[cfg(feature = "tauri")]
impl AppEmitter for TauriEmitter {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        use tauri::Emitter;
        self.0
            .emit(event, payload)
            .map_err(|error| error.to_string())
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
