//! Who answers in a room when nothing else says who (`team_room::route`'s
//! `lead`): set by the person from the room's head. Local only, like the
//! rest of the agents' routes.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LeadInput {
    /// The project whose room it is; none: the Geral room.
    #[serde(default)]
    room: Option<String>,
    /// The agent that answers by default; none: DCC's own pick.
    #[serde(default)]
    agent_id: Option<String>,
}

pub(super) async fn set(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<LeadInput>,
) -> Result<Json<Value>, HttpApiError> {
    let room = input
        .room
        .as_deref()
        .map(str::trim)
        .filter(|room| !room.is_empty());
    backend
        .sessions
        .set_room_lead(room, input.agent_id.as_deref())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}
