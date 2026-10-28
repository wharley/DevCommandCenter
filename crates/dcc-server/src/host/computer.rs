//! Computer use for the native app (`http_api::computer_bridge`): the same
//! state and answers as the desktop's dialog.

use serde_json::Value;

use crate::named_events::NamedEvents;

#[derive(Clone)]
pub struct HostComputer {
    pub computer: crate::computer_use_commands::ComputerUseState,
    pub requests: crate::desktop_agent_requests::DesktopAgentRequests,
    pub events: NamedEvents,
}

impl crate::http_api::DesktopComputer for HostComputer {
    fn pending(&self) -> Value {
        let state = &self.computer;
        serde_json::to_value(state.pending_control_requests().requests)
            .unwrap_or_else(|_| Value::Array(Vec::new()))
    }

    fn status(
        &self,
        session_id: Option<String>,
        include_targets: bool,
    ) -> crate::http_api::BrowserFuture<Value> {
        let state = self.computer.clone();
        Box::pin(async move {
            // Lists the Mac's windows: off the async threads.
            let status = tokio::task::spawn_blocking(move || {
                state.status(session_id.as_deref(), include_targets)
            })
            .await
            .map_err(|_| "desktop computer-use status failed".to_string())?;
            serde_json::to_value(status).map_err(|error| error.to_string())
        })
    }

    fn request_access(&self, kind: String) -> crate::http_api::BrowserFuture<Value> {
        let state = self.computer.clone();
        Box::pin(async move {
            let kind: crate::computer_use_commands::ComputerAccessKind =
                serde_json::from_value(Value::String(kind))
                    .map_err(|_| "desktop computer-use access kind is invalid".to_string())?;
            let status = tokio::task::spawn_blocking(move || state.request_access(kind))
                .await
                .map_err(|_| "desktop computer-use access failed".to_string())??;
            serde_json::to_value(status).map_err(|error| error.to_string())
        })
    }

    fn preview(&self, session_id: String, refresh: bool) -> crate::http_api::BrowserFuture<Value> {
        let state = self.computer.clone();
        Box::pin(async move {
            let frame = if refresh {
                tokio::task::spawn_blocking(move || state.refresh_preview_frame(&session_id))
                    .await
                    .map_err(|_| "desktop computer-use preview refresh failed".to_string())?
            } else {
                state.preview_frame(&session_id)
            };
            serde_json::to_value(frame).map_err(|error| error.to_string())
        })
    }

    fn disarm(&self, session_id: String) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            let state = &this.computer;
            let grant = state.disarm(&session_id);
            crate::computer_use_commands::publish_preview_cleared(&session_id);
            let _ = this.events.emit("computer-use-preview-cleared", session_id);
            serde_json::to_value(grant).map_err(|error| error.to_string())
        })
    }

    fn respond(
        &self,
        request_id: String,
        allowed: bool,
        allowed_bundle_ids: Vec<String>,
    ) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            let output = crate::computer_use_commands::computer_use_respond_control_request(
                &this.requests,
                crate::computer_use_commands::ComputerUseRespondControlRequestInput {
                    request_id,
                    allowed,
                    allowed_bundle_ids,
                },
            )
            .await?;
            serde_json::to_value(output).map_err(|error| error.to_string())
        })
    }
}
