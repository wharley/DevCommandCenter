//! Events by name for whoever shows the app (`terminal-exit`,
//! `computer-use-preview-cleared`, the Browser's open approvals): the
//! desktop's webview through its emitter, or none.

use std::sync::Arc;

use dcc_tauri::app_events::AppEmitter;
use serde::Serialize;

#[derive(Clone)]
pub struct NamedEvents(pub Arc<dyn AppEmitter>);

impl NamedEvents {
    pub fn emit<T: Serialize>(&self, event: &str, payload: T) -> Result<(), String> {
        let payload = serde_json::to_value(payload).map_err(|error| error.to_string())?;
        self.0.emit(event, payload)
    }
}
