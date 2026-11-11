//! An approval answered from the phone's room with a reason: denied with
//! what the person wrote, which Claude reads with the denial; any other
//! provider gets it as its next message, queued for when its turn ends —
//! the desktop room's own answer (`dcc_core::domain::room_approval`).

use dcc_core::domain::room_approval::{denial_reason_message, reads_denial_reason, short_tool};
use dcc_tauri::commands::session_commands::RespondToPermissionRequestInput;

use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ApprovalAnswer {
    session_id: String,
    request_id: String,
    /// `allow` or `deny`.
    behavior: String,
    /// Why the person denied it.
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    always: bool,
    /// The tool asked for, to name it in a reason sent as a message.
    #[serde(default)]
    tool_name: Option<String>,
}

/// The message a reason goes as, when the provider cannot read it with the
/// denial; none when it can, or when nothing was written.
pub(super) fn reason_turn(
    provider_id: &str,
    behavior: &str,
    tool: &str,
    reason: Option<&str>,
) -> Option<String> {
    let reason = reason.map(str::trim).filter(|reason| !reason.is_empty())?;
    (behavior == "deny" && !reads_denial_reason(provider_id))
        .then(|| denial_reason_message(short_tool(tool), reason))
}

/// `POST /api/v1/mobile/room/approvals/respond`:
/// `{sessionId, requestId, behavior, message?, always?, toolName?}`.
pub(super) async fn respond(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    State(backend): State<Arc<HttpBackend>>,
    Json(answer): Json<ApprovalAnswer>,
) -> Result<Json<Value>, HttpApiError> {
    let state = &backend.sessions;
    let session_id = SessionId(answer.session_id.clone());
    let provider = state
        .peek_session(&session_id)
        .await
        .ok()
        .flatten()
        .map(|session| session.provider_id)
        .unwrap_or_default();
    let message = answer
        .message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_string);
    let output = dcc_tauri::commands::session_commands::respond_to_permission_request_with_state(
        state,
        RespondToPermissionRequestInput {
            session_id: answer.session_id.clone(),
            request_id: answer.request_id.clone(),
            behavior: answer.behavior.clone(),
            message: message.clone(),
            always: answer.always,
        },
    )
    .await
    .map_err(classify_respond_error)?;
    let mut value =
        serde_json::to_value(output).map_err(|error| HttpApiError::internal(error.to_string()))?;
    let tool = answer.tool_name.unwrap_or_default();
    let mut queued = false;
    if let Some(prompt) = reason_turn(&provider, &answer.behavior, &tool, message.as_deref()) {
        let mut input: SendTurnInput =
            serde_json::from_value(json!({ "sessionId": answer.session_id, "prompt": prompt }))
                .map_err(|error| HttpApiError::internal(error.to_string()))?;
        mobile::resolve_turn_approval_policy(config, &backend, &mut input).await?;
        // The denial stands even when the reason cannot go.
        queued = dcc_tauri::commands::session_commands::queue_turn_with_state(
            state,
            dcc_core::application::QueueTurnInput { turn: input },
        )
        .await
        .is_ok();
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("reasonQueued".into(), json!(queued));
    }
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reason_goes_as_a_message_only_where_the_denial_cannot_carry_it() {
        assert_eq!(
            reason_turn(
                "codex",
                "deny",
                "mcp__dcc-slack__slack_send_message",
                Some(" não manda ")
            ),
            Some("[DCC] I denied `slack_send_message`: não manda".to_string())
        );
        assert_eq!(
            reason_turn("claude_code", "deny", "Bash", Some("não")),
            None
        );
        assert_eq!(reason_turn("codex", "allow", "Bash", Some("ok")), None);
        assert_eq!(reason_turn("codex", "deny", "Bash", Some("  ")), None);
        assert_eq!(reason_turn("codex", "deny", "Bash", None), None);
    }
}
