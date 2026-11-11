//! "Enviar" / "Descartar" on a reply an agent proposed (`agent_reply`), the
//! same for the desktop's room and the phone's (`/api/v1/mobile/room/
//! replies`, as the relay never forwards `/api/v1/agents/`). Sending calls
//! the connector's tool with no model (`dcc_tauri::reply_send`).
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ReplyAction {
    Send,
    Discard,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReplyInput {
    id: String,
    action: ReplyAction,
    /// The text as the person left it; missing: as the agent wrote it.
    #[serde(default)]
    text: Option<String>,
}

/// `POST /api/v1/mobile/room/replies`
pub(super) async fn answer(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ReplyInput>,
) -> Result<Json<Value>, HttpApiError> {
    let id = input.id.trim();
    let sessions = &backend.sessions;
    match input.action {
        ReplyAction::Discard => {
            let changed = sessions
                .discard_agent_reply(id)
                .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
            if !changed {
                return Err(HttpApiError::not_found("reply not found"));
            }
        }
        ReplyAction::Send => {
            let reply = sessions
                .agent_reply(id)
                .map_err(|error| HttpApiError::bad_request(error.to_string()))?
                .ok_or_else(|| HttpApiError::not_found("reply not found"))?;
            let gateway = backend
                .app
                .get()
                .and_then(|app| app.connectors())
                .map(|(_, gateway)| gateway)
                .ok_or_else(|| HttpApiError::not_found("connectors are not available"))?;
            let text = input.text.unwrap_or(reply.text);
            dcc_tauri::reply_send::send_reply(sessions, &gateway, id, &text)
                .await
                .map_err(HttpApiError::bad_request)?;
        }
    }
    Ok(Json(json!({ "ok": true })))
}
